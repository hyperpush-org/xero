use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use regex::Regex;
use reqwest::Url;
#[cfg(any(windows, test))]
use serde::Deserialize;

use super::{
    policy::{process_manager_policy_trace, CommandPolicyDecision, PreparedCommandRequest},
    process::{apply_sanitized_command_environment, SAFE_COMMAND_ENV_KEYS},
    repo_scope::{display_relative_or_root, normalize_relative_path},
    AutonomousCommandPolicyOutcome, AutonomousCommandPolicyTrace, AutonomousProcessActionRiskLevel,
    AutonomousProcessCommandMetadata, AutonomousProcessHighlight, AutonomousProcessHighlightKind,
    AutonomousProcessLifecycleContract, AutonomousProcessManagerAction,
    AutonomousProcessManagerContract, AutonomousProcessManagerOutput,
    AutonomousProcessManagerPolicyTrace, AutonomousProcessManagerRequest,
    AutonomousProcessMetadata, AutonomousProcessOutputArtifact, AutonomousProcessOutputChunk,
    AutonomousProcessOutputLimits, AutonomousProcessOutputStream, AutonomousProcessOwner,
    AutonomousProcessOwnershipScope, AutonomousProcessPersistenceContract,
    AutonomousProcessReadinessDetector, AutonomousProcessReadinessState, AutonomousProcessStatus,
    AutonomousProcessStdinState, AutonomousSystemPort, AutonomousToolOutput, AutonomousToolResult,
    AutonomousToolRuntime, AUTONOMOUS_TOOL_PROCESS_MANAGER,
};
use crate::{
    auth::now_timestamp,
    commands::{validate_non_empty, CommandError, CommandResult},
    db::project_app_data_dir_for_repo,
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

const PROCESS_MANAGER_PHASE: &str = "phase_5_system_process_visibility";
const PROCESS_MANAGER_INITIAL_DRAIN: Duration = Duration::from_millis(150);
const PROCESS_MANAGER_SEND_DRAIN: Duration = Duration::from_millis(50);
const PROCESS_MANAGER_WAIT_POLL: Duration = Duration::from_millis(25);
const PROCESS_MANAGER_HTTP_PROBE_TIMEOUT: Duration = Duration::from_millis(300);
const MAX_OWNED_PROCESSES: usize = 8;
const RECENT_OUTPUT_RING_BYTES: usize = 1024 * 1024;
const RECENT_OUTPUT_RING_CHUNKS: usize = 512;
const FULL_OUTPUT_ARTIFACT_THRESHOLD_BYTES: usize = 1024 * 1024;
const PROCESS_OUTPUT_EXCERPT_BYTES: usize = 16 * 1024;
const MAX_PROCESS_OUTPUT_READ_BYTES: usize = 64 * 1024;
const MAX_PROCESS_OUTPUT_TAIL_LINES: usize = 200;
const MAX_PROCESS_STDIN_INPUT_BYTES: usize = 64 * 1024;
const MAX_PROCESS_HIGHLIGHTS: usize = 32;
const MAX_SYSTEM_PROCESS_RESULTS: usize = 200;
const MAX_SYSTEM_TREE_PROCESSES: usize = 512;
const MAX_SYSTEM_PORT_RESULTS: usize = 200;
const ASYNC_JOB_ARTIFACT_DIR: &str = "tool-artifacts/process";
const REDACTED_PROCESS_OUTPUT_SUMMARY: &str =
    "Process output was redacted before durable persistence.";
const INTERNAL_MARKER_PREFIX: &str = "__XERO_";

#[derive(Debug, Default)]
pub(super) struct OwnedProcessRegistry {
    processes: Mutex<BTreeMap<String, Arc<OwnedProcess>>>,
    next_id: AtomicU64,
}

impl OwnedProcessRegistry {
    fn next_process_id(&self) -> String {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        format!("owned-process-{id}")
    }

    fn ensure_capacity(&self) -> CommandResult<()> {
        self.remove_completed_async_jobs()?;
        let processes = self.processes.lock().map_err(process_registry_lock_error)?;
        if processes.len() >= MAX_OWNED_PROCESSES {
            return Err(CommandError::user_fixable(
                "autonomous_tool_process_manager_limit_reached",
                format!(
                    "Xero limits the process manager to {MAX_OWNED_PROCESSES} concurrent owned process(es). Kill an existing process before starting another."
                ),
            ));
        }
        Ok(())
    }

    fn insert(&self, process: Arc<OwnedProcess>) -> CommandResult<()> {
        self.remove_completed_async_jobs()?;
        let mut processes = self.processes.lock().map_err(process_registry_lock_error)?;
        if processes.len() >= MAX_OWNED_PROCESSES {
            return Err(CommandError::user_fixable(
                "autonomous_tool_process_manager_limit_reached",
                format!(
                    "Xero limits the process manager to {MAX_OWNED_PROCESSES} concurrent owned process(es). Kill an existing process before starting another."
                ),
            ));
        }
        processes.insert(process.process_id.clone(), process);
        Ok(())
    }

    fn get(&self, process_id: &str) -> CommandResult<Arc<OwnedProcess>> {
        let processes = self.processes.lock().map_err(process_registry_lock_error)?;
        processes.get(process_id).cloned().ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_tool_process_manager_not_found",
                format!("Xero could not find owned process `{process_id}`."),
            )
        })
    }

    fn list(&self) -> CommandResult<Vec<Arc<OwnedProcess>>> {
        let processes = self.processes.lock().map_err(process_registry_lock_error)?;
        Ok(processes.values().cloned().collect())
    }

    fn remove(&self, process_id: &str) -> CommandResult<Arc<OwnedProcess>> {
        let mut processes = self.processes.lock().map_err(process_registry_lock_error)?;
        processes.remove(process_id).ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_tool_process_manager_not_found",
                format!("Xero could not find owned process `{process_id}`."),
            )
        })
    }

    fn remove_completed_async_jobs(&self) -> CommandResult<()> {
        let processes = self.list()?;
        let mut completed = Vec::new();
        for process in processes {
            let _ = process.poll_exit()?;
            if process.is_async_job() && process.is_terminal()? {
                completed.push(process.process_id.clone());
            }
        }
        if completed.is_empty() {
            return Ok(());
        }

        let mut processes = self.processes.lock().map_err(process_registry_lock_error)?;
        for process_id in completed {
            processes.remove(&process_id);
        }
        Ok(())
    }
}

impl Drop for OwnedProcessRegistry {
    fn drop(&mut self) {
        if let Ok(processes) = self.processes.get_mut() {
            for process in processes.values() {
                let _ = process.kill();
            }
        }
    }
}

fn process_registry_lock_error(_error: std::sync::PoisonError<impl Sized>) -> CommandError {
    CommandError::system_fault(
        "autonomous_tool_process_manager_lock_failed",
        "Xero could not lock the owned process registry.",
    )
}

#[derive(Debug)]
struct OwnedProcess {
    process_id: String,
    pid: u32,
    label: Option<String>,
    process_type: Option<String>,
    group: Option<String>,
    owner: AutonomousProcessOwner,
    launch_config: OwnedProcessLaunchConfig,
    command: AutonomousProcessCommandMetadata,
    stdin: Mutex<Option<ChildStdin>>,
    stdin_state: Mutex<AutonomousProcessStdinState>,
    child: Mutex<Option<Child>>,
    status: Mutex<AutonomousProcessStatus>,
    readiness: Mutex<AutonomousProcessReadinessState>,
    started_at: String,
    exited_at: Mutex<Option<String>>,
    exit_code: Mutex<Option<i32>>,
    chunks: Mutex<Vec<AutonomousProcessOutputChunk>>,
    raw_chunks: Mutex<Vec<RawProcessOutputChunk>>,
    durable_output: Mutex<DurableProcessOutput>,
    output_artifact: Mutex<Option<AutonomousProcessOutputArtifact>>,
    next_cursor: AtomicU64,
    last_read_cursor: AtomicU64,
    restart_count: AtomicU32,
    last_restart_reason: Mutex<Option<String>>,
}

#[derive(Debug, Clone)]
struct RawProcessOutputChunk {
    cursor: u64,
    stream: AutonomousProcessOutputStream,
    text: String,
    captured_at: Option<String>,
}

#[derive(Debug, Default, Clone)]
struct DurableProcessOutput {
    text: String,
    redacted: bool,
}

#[derive(Debug, Clone)]
struct OwnedProcessLaunchConfig {
    prepared: PreparedCommandRequest,
    shell_mode: bool,
    interactive: bool,
    label: Option<String>,
    process_type: Option<String>,
    group: Option<String>,
    persistent: bool,
    async_job: bool,
    timeout_ms: Option<u64>,
    output_artifact_dir: PathBuf,
}

impl OwnedProcess {
    #[allow(clippy::too_many_arguments)]
    fn new(
        process_id: String,
        launch_config: OwnedProcessLaunchConfig,
        child: Child,
        stdin: Option<ChildStdin>,
        restart_count: u32,
        last_restart_reason: Option<String>,
    ) -> Self {
        let pid = child.id();
        let command = AutonomousProcessCommandMetadata {
            argv: redact_command_argv_for_persistence(&launch_config.prepared.argv),
            shell_mode: launch_config.shell_mode,
            cwd: display_relative_or_root(&launch_config.prepared.cwd, &launch_config.prepared.cwd),
            sanitized_env: sanitized_env_summary(),
        };
        Self {
            process_id,
            pid,
            label: launch_config.label.clone(),
            process_type: launch_config.process_type.clone(),
            group: launch_config.group.clone(),
            owner: AutonomousProcessOwner {
                thread_id: None,
                session_id: None,
                repo_id: None,
                user_id: None,
                scope: AutonomousProcessOwnershipScope::XeroOwned,
            },
            launch_config,
            command,
            stdin_state: Mutex::new(if stdin.is_some() {
                AutonomousProcessStdinState::Open
            } else {
                AutonomousProcessStdinState::Unavailable
            }),
            stdin: Mutex::new(stdin),
            child: Mutex::new(Some(child)),
            status: Mutex::new(AutonomousProcessStatus::Running),
            readiness: Mutex::new(AutonomousProcessReadinessState {
                ready: false,
                detector: None,
                matched: None,
            }),
            started_at: now_timestamp(),
            exited_at: Mutex::new(None),
            exit_code: Mutex::new(None),
            chunks: Mutex::new(Vec::new()),
            raw_chunks: Mutex::new(Vec::new()),
            durable_output: Mutex::new(DurableProcessOutput::default()),
            output_artifact: Mutex::new(None),
            next_cursor: AtomicU64::new(1),
            last_read_cursor: AtomicU64::new(0),
            restart_count: AtomicU32::new(restart_count),
            last_restart_reason: Mutex::new(last_restart_reason),
        }
    }

    fn set_display_cwd(&mut self, cwd: String) {
        self.command.cwd = cwd;
    }

    fn push_chunk(
        &self,
        stream: AutonomousProcessOutputStream,
        capture: SanitizedProcessOutput,
        raw_text: Option<String>,
    ) -> CommandResult<()> {
        let cursor = self.next_cursor.fetch_add(1, Ordering::Relaxed);
        let captured_at = Some(now_timestamp());
        if self.launch_config.async_job {
            self.append_durable_output(&capture)?;
        }
        if let Some(text) = raw_text.filter(|text| !text.trim().is_empty()) {
            let mut raw_chunks = self.raw_chunks.lock().map_err(process_output_lock_error)?;
            raw_chunks.push(RawProcessOutputChunk {
                cursor,
                stream,
                text,
                captured_at: captured_at.clone(),
            });
            prune_raw_process_output_chunks(&mut raw_chunks);
        }
        let mut chunks = self.chunks.lock().map_err(process_output_lock_error)?;
        chunks.push(AutonomousProcessOutputChunk {
            cursor,
            stream,
            text: capture.text,
            truncated: capture.truncated,
            redacted: capture.redacted,
            captured_at,
        });
        prune_process_output_chunks(&mut chunks);
        Ok(())
    }

    fn append_durable_output(&self, capture: &SanitizedProcessOutput) -> CommandResult<()> {
        let Some(text) = capture.text.as_deref() else {
            return Ok(());
        };
        let mut durable = self
            .durable_output
            .lock()
            .map_err(process_output_lock_error)?;
        durable.redacted |= capture.redacted;
        if !durable.text.is_empty() && !durable.text.ends_with('\n') {
            durable.text.push('\n');
        }
        durable.text.push_str(text);
        if !text.ends_with('\n') {
            durable.text.push('\n');
        }
        Ok(())
    }

    fn read_chunks_after(
        &self,
        after_cursor: u64,
        max_bytes: usize,
    ) -> CommandResult<Vec<AutonomousProcessOutputChunk>> {
        Ok(self
            .read_chunks_after_raw(after_cursor, max_bytes)?
            .into_iter()
            .map(filter_internal_marker_chunk)
            .collect())
    }

    fn read_chunks_after_raw(
        &self,
        after_cursor: u64,
        max_bytes: usize,
    ) -> CommandResult<Vec<AutonomousProcessOutputChunk>> {
        let chunks = self.chunks.lock().map_err(process_output_lock_error)?;
        let mut selected = Vec::new();
        let mut bytes = 0_usize;
        for chunk in chunks
            .iter()
            .filter(|chunk| chunk.cursor > after_cursor)
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

    fn next_cursor_value(&self) -> u64 {
        self.next_cursor.load(Ordering::Relaxed)
    }

    fn read_raw_chunks_after(
        &self,
        after_cursor: u64,
        max_bytes: usize,
    ) -> CommandResult<Vec<RawProcessOutputChunk>> {
        let chunks = self.raw_chunks.lock().map_err(process_output_lock_error)?;
        let mut selected = Vec::new();
        let mut bytes = 0_usize;
        for chunk in chunks
            .iter()
            .filter(|chunk| chunk.cursor > after_cursor)
            .cloned()
        {
            let chunk_bytes = chunk.text.len();
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

    fn last_read_cursor_value(&self) -> u64 {
        self.last_read_cursor.load(Ordering::Relaxed)
    }

    fn remember_last_read_cursor(&self, cursor: u64) {
        self.last_read_cursor.store(cursor, Ordering::Relaxed);
    }

    fn launch_config(&self) -> OwnedProcessLaunchConfig {
        self.launch_config.clone()
    }

    fn restart_count_value(&self) -> u32 {
        self.restart_count.load(Ordering::Relaxed)
    }

    fn is_async_job(&self) -> bool {
        self.launch_config.async_job
    }

    fn is_terminal(&self) -> CommandResult<bool> {
        let status = *self.status.lock().map_err(process_status_lock_error)?;
        Ok(matches!(
            status,
            AutonomousProcessStatus::Exited
                | AutonomousProcessStatus::Failed
                | AutonomousProcessStatus::Killed
        ))
    }

    fn mark_ready(
        &self,
        detector: AutonomousProcessReadinessDetector,
        matched: String,
    ) -> CommandResult<()> {
        *self
            .readiness
            .lock()
            .map_err(process_readiness_lock_error)? = AutonomousProcessReadinessState {
            ready: true,
            detector: Some(detector),
            matched: Some(matched),
        };
        let mut status = self.status.lock().map_err(process_status_lock_error)?;
        if !matches!(
            *status,
            AutonomousProcessStatus::Exited
                | AutonomousProcessStatus::Failed
                | AutonomousProcessStatus::Killing
                | AutonomousProcessStatus::Killed
        ) {
            *status = AutonomousProcessStatus::Ready;
        }
        Ok(())
    }

    fn close_stdin(&self) -> CommandResult<()> {
        let mut stdin = self.stdin.lock().map_err(process_stdin_lock_error)?;
        if stdin.take().is_some() {
            *self.stdin_state.lock().map_err(process_stdin_lock_error)? =
                AutonomousProcessStdinState::Closed;
        }
        Ok(())
    }

    fn send_input(&self, input: &str) -> CommandResult<()> {
        if self.poll_exit()?.is_some() {
            let _ = self.close_stdin();
            return Err(CommandError::user_fixable(
                "autonomous_tool_process_manager_stdin_closed",
                format!(
                    "Xero cannot send stdin to owned process `{}` because it has exited.",
                    self.process_id
                ),
            ));
        }

        let mut stdin = self.stdin.lock().map_err(process_stdin_lock_error)?;
        let Some(stdin_ref) = stdin.as_mut() else {
            let state = *self.stdin_state.lock().map_err(process_stdin_lock_error)?;
            return Err(CommandError::user_fixable(
                "autonomous_tool_process_manager_stdin_unavailable",
                format!(
                    "Xero cannot send stdin to owned process `{}` because stdin is {state:?}. Start the process with interactive=true or shellMode=true.",
                    self.process_id
                ),
            ));
        };

        stdin_ref.write_all(input.as_bytes()).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_process_manager_stdin_write_failed",
                format!(
                    "Xero could not write stdin to owned process `{}`: {error}",
                    self.process_id
                ),
            )
        })?;
        stdin_ref.flush().map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_process_manager_stdin_flush_failed",
                format!(
                    "Xero could not flush stdin for owned process `{}`: {error}",
                    self.process_id
                ),
            )
        })?;
        Ok(())
    }

    fn poll_exit(&self) -> CommandResult<Option<i32>> {
        if let Some(exit_code) = *self.exit_code.lock().map_err(process_exit_lock_error)? {
            return Ok(Some(exit_code));
        }

        let mut child = self.child.lock().map_err(process_state_lock_error)?;
        let Some(child_ref) = child.as_mut() else {
            return Ok(*self.exit_code.lock().map_err(process_exit_lock_error)?);
        };

        match child_ref.try_wait() {
            Ok(Some(status)) => {
                let exit_code = status.code();
                cleanup_process_group_after_root_exit(child_ref.id());
                *self.exit_code.lock().map_err(process_exit_lock_error)? = exit_code;
                *self.exited_at.lock().map_err(process_exit_lock_error)? = Some(now_timestamp());
                *self.status.lock().map_err(process_status_lock_error)? =
                    AutonomousProcessStatus::Exited;
                let _ = self.close_stdin();
                *child = None;
                Ok(exit_code)
            }
            Ok(None) => {
                let mut status = self.status.lock().map_err(process_status_lock_error)?;
                if !matches!(*status, AutonomousProcessStatus::Ready) {
                    *status = AutonomousProcessStatus::Running;
                }
                Ok(None)
            }
            Err(error) => Err(CommandError::retryable(
                "autonomous_tool_process_manager_wait_failed",
                format!(
                    "Xero could not observe owned process `{}`: {error}",
                    self.process_id
                ),
            )),
        }
    }

    fn kill(&self) -> CommandResult<Option<i32>> {
        let mut child = self.child.lock().map_err(process_state_lock_error)?;
        let Some(child_ref) = child.as_mut() else {
            return Ok(*self.exit_code.lock().map_err(process_exit_lock_error)?);
        };

        match child_ref.try_wait() {
            Ok(Some(status)) => {
                let exit_code = status.code();
                cleanup_process_group_after_root_exit(child_ref.id());
                *self.exit_code.lock().map_err(process_exit_lock_error)? = exit_code;
                *self.exited_at.lock().map_err(process_exit_lock_error)? = Some(now_timestamp());
                *self.status.lock().map_err(process_status_lock_error)? =
                    AutonomousProcessStatus::Exited;
                let _ = self.close_stdin();
                *child = None;
                Ok(exit_code)
            }
            Ok(None) => {
                *self.status.lock().map_err(process_status_lock_error)? =
                    AutonomousProcessStatus::Killing;
                let _ = self.close_stdin();
                let status = terminate_process_tree(child_ref).map_err(|error| {
                    CommandError::retryable(
                        "autonomous_tool_process_manager_kill_failed",
                        format!(
                            "Xero could not kill owned process `{}`: {error}",
                            self.process_id
                        ),
                    )
                })?;
                let exit_code = status.code();
                *self.exit_code.lock().map_err(process_exit_lock_error)? = exit_code;
                *self.exited_at.lock().map_err(process_exit_lock_error)? = Some(now_timestamp());
                *self.status.lock().map_err(process_status_lock_error)? =
                    AutonomousProcessStatus::Killed;
                *child = None;
                Ok(exit_code)
            }
            Err(error) => Err(CommandError::retryable(
                "autonomous_tool_process_manager_wait_failed",
                format!(
                    "Xero could not observe owned process `{}` before killing it: {error}",
                    self.process_id
                ),
            )),
        }
    }

    fn ensure_output_artifact(
        &self,
        status: AutonomousProcessStatus,
    ) -> CommandResult<Option<AutonomousProcessOutputArtifact>> {
        if !self.launch_config.async_job
            || !matches!(
                status,
                AutonomousProcessStatus::Exited
                    | AutonomousProcessStatus::Failed
                    | AutonomousProcessStatus::Killed
            )
        {
            return self
                .output_artifact
                .lock()
                .map_err(process_output_lock_error)
                .map(|artifact| artifact.clone());
        }

        let mut artifact = self
            .output_artifact
            .lock()
            .map_err(process_output_lock_error)?;
        let durable = self
            .durable_output
            .lock()
            .map_err(process_output_lock_error)?
            .clone();
        let text = filter_internal_marker_text(&durable.text);
        let redacted = durable.redacted;

        let dir = self.launch_config.output_artifact_dir.clone();
        fs::create_dir_all(&dir).map_err(|error| {
            CommandError::system_fault(
                "autonomous_tool_process_manager_artifact_failed",
                format!(
                    "Xero could not create async job artifact directory {}: {error}",
                    dir.display()
                ),
            )
        })?;
        let path = artifact
            .as_ref()
            .map(|artifact| PathBuf::from(&artifact.path))
            .unwrap_or_else(|| self.async_job_artifact_path(&dir));
        fs::write(&path, text.as_bytes()).map_err(|error| {
            CommandError::system_fault(
                "autonomous_tool_process_manager_artifact_failed",
                format!(
                    "Xero could not write async job artifact {}: {error}",
                    path.display()
                ),
            )
        })?;

        *artifact = Some(AutonomousProcessOutputArtifact {
            path: path.display().to_string(),
            byte_count: text.len(),
            redacted,
        });
        Ok(artifact.clone())
    }

    fn async_job_artifact_path(&self, dir: &std::path::Path) -> PathBuf {
        dir.join(format!(
            "{}-{}-{}.log",
            marker_safe(&self.process_id),
            self.pid,
            marker_safe(&self.started_at)
        ))
    }

    fn metadata(&self) -> CommandResult<AutonomousProcessMetadata> {
        let exit_code = *self.exit_code.lock().map_err(process_exit_lock_error)?;
        let status = *self.status.lock().map_err(process_status_lock_error)?;
        let stdin_state = *self.stdin_state.lock().map_err(process_stdin_lock_error)?;
        let output_artifact = self.ensure_output_artifact(status)?;
        let last_restart_reason = self
            .last_restart_reason
            .lock()
            .map_err(process_state_lock_error)?
            .clone();
        let readiness = self
            .readiness
            .lock()
            .map_err(process_readiness_lock_error)?
            .clone();
        let exited_at = self
            .exited_at
            .lock()
            .map_err(process_exit_lock_error)?
            .clone();
        let chunks = self.retained_chunks()?;
        let raw_chunks = self.read_raw_chunks_after(0, RECENT_OUTPUT_RING_BYTES)?;
        let mut highlights = extract_process_highlights(&self.process_id, &chunks);
        highlights.extend(extract_process_network_highlights_from_raw(
            &self.process_id,
            &raw_chunks,
        ));
        let highlights = truncate_highlights(highlights);
        let detected_urls =
            unique_highlight_texts(&highlights, AutonomousProcessHighlightKind::Url);
        let detected_ports = unique_highlight_ports(&highlights);
        let recent_errors =
            recent_highlight_texts(&highlights, AutonomousProcessHighlightKind::Error);
        let recent_warnings =
            recent_highlight_texts(&highlights, AutonomousProcessHighlightKind::Warning);
        let recent_stack_traces =
            recent_highlight_texts(&highlights, AutonomousProcessHighlightKind::StackTrace);
        let status_changes =
            process_status_summaries(&self.process_id, status, exit_code, readiness.clone());
        Ok(AutonomousProcessMetadata {
            process_id: self.process_id.clone(),
            pid: Some(self.pid),
            parent_pid: None,
            process_group_id: Some(self.pid as i64),
            process_name: self.command.argv.first().cloned(),
            executable_path: None,
            label: self.label.clone(),
            process_type: self.process_type.clone(),
            group: self.group.clone(),
            owner: self.owner.clone(),
            command: self.command.clone(),
            stdin_state,
            status,
            started_at: Some(self.started_at.clone()),
            exited_at,
            exit_code,
            output_cursor: Some(self.next_cursor_value().saturating_sub(1)),
            detected_urls,
            detected_ports,
            recent_errors,
            recent_warnings,
            recent_stack_traces,
            status_changes,
            readiness,
            restart_count: self.restart_count_value(),
            last_restart_reason,
            async_job: self.launch_config.async_job,
            timeout_ms: self.launch_config.timeout_ms,
            output_artifact,
        })
    }

    fn retained_chunks(&self) -> CommandResult<Vec<AutonomousProcessOutputChunk>> {
        let chunks = self.chunks.lock().map_err(process_output_lock_error)?;
        Ok(chunks
            .iter()
            .cloned()
            .map(filter_internal_marker_chunk)
            .collect())
    }
}

fn process_state_lock_error(_error: std::sync::PoisonError<impl Sized>) -> CommandError {
    CommandError::system_fault(
        "autonomous_tool_process_manager_lock_failed",
        "Xero could not lock owned process state.",
    )
}

fn process_status_lock_error(_error: std::sync::PoisonError<impl Sized>) -> CommandError {
    CommandError::system_fault(
        "autonomous_tool_process_manager_lock_failed",
        "Xero could not lock owned process status.",
    )
}

fn process_readiness_lock_error(_error: std::sync::PoisonError<impl Sized>) -> CommandError {
    CommandError::system_fault(
        "autonomous_tool_process_manager_lock_failed",
        "Xero could not lock owned process readiness state.",
    )
}

fn process_exit_lock_error(_error: std::sync::PoisonError<impl Sized>) -> CommandError {
    CommandError::system_fault(
        "autonomous_tool_process_manager_lock_failed",
        "Xero could not lock owned process exit state.",
    )
}

fn process_output_lock_error(_error: std::sync::PoisonError<impl Sized>) -> CommandError {
    CommandError::system_fault(
        "autonomous_tool_process_manager_lock_failed",
        "Xero could not lock owned process output.",
    )
}

fn process_stdin_lock_error(_error: std::sync::PoisonError<impl Sized>) -> CommandError {
    CommandError::system_fault(
        "autonomous_tool_process_manager_lock_failed",
        "Xero could not lock owned process stdin.",
    )
}

struct SpawnOwnedProcessOptions {
    action: AutonomousProcessManagerAction,
    process_id: Option<String>,
    restart_count: u32,
    last_restart_reason: Option<String>,
    async_job: bool,
}

impl SpawnOwnedProcessOptions {
    fn new(action: AutonomousProcessManagerAction) -> Self {
        Self {
            action,
            process_id: None,
            restart_count: 0,
            last_restart_reason: None,
            async_job: false,
        }
    }

    fn with_process_id(mut self, process_id: String) -> Self {
        self.process_id = Some(process_id);
        self
    }

    fn with_restart(mut self, restart_count: u32, reason: Option<String>) -> Self {
        self.restart_count = restart_count;
        self.last_restart_reason = reason;
        self
    }

    fn with_async_job(mut self, async_job: bool) -> Self {
        self.async_job = async_job;
        self
    }
}

impl AutonomousToolRuntime {
    pub fn process_manager(
        &self,
        request: AutonomousProcessManagerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.process_manager_with_approval(request, false)
    }

    pub fn process_manager_with_operator_approval(
        &self,
        request: AutonomousProcessManagerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.process_manager_with_approval(request, true)
    }

    fn process_manager_with_approval(
        &self,
        request: AutonomousProcessManagerRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        validate_process_manager_request(&request)?;
        validate_phase_5_scope(&request)?;

        match request.action {
            AutonomousProcessManagerAction::Start => {
                self.process_manager_start(request, operator_approved)
            }
            AutonomousProcessManagerAction::AsyncStart => {
                self.process_manager_async_start(request, operator_approved)
            }
            AutonomousProcessManagerAction::List => self.process_manager_list(request),
            AutonomousProcessManagerAction::Status => self.process_manager_status(request),
            AutonomousProcessManagerAction::Output => self.process_manager_output(request),
            AutonomousProcessManagerAction::Digest => self.process_manager_digest(request),
            AutonomousProcessManagerAction::WaitForReady => {
                self.process_manager_wait_for_ready(request)
            }
            AutonomousProcessManagerAction::Highlights => self.process_manager_highlights(request),
            AutonomousProcessManagerAction::Send => {
                self.process_manager_send(request, operator_approved)
            }
            AutonomousProcessManagerAction::SendAndWait => {
                self.process_manager_send_and_wait(request, operator_approved)
            }
            AutonomousProcessManagerAction::Run => {
                self.process_manager_run(request, operator_approved)
            }
            AutonomousProcessManagerAction::Env => self.process_manager_env(request),
            AutonomousProcessManagerAction::Kill => self.process_manager_kill(request),
            AutonomousProcessManagerAction::Restart => {
                self.process_manager_restart(request, operator_approved)
            }
            AutonomousProcessManagerAction::GroupStatus => {
                self.process_manager_group_status(request)
            }
            AutonomousProcessManagerAction::GroupKill => self.process_manager_group_kill(request),
            AutonomousProcessManagerAction::AsyncAwait => self.process_manager_async_await(request),
            AutonomousProcessManagerAction::AsyncCancel => {
                self.process_manager_async_cancel(request)
            }
            AutonomousProcessManagerAction::SystemProcessList => {
                self.process_manager_system_process_list(request)
            }
            AutonomousProcessManagerAction::SystemProcessTree => {
                self.process_manager_system_process_tree(request)
            }
            AutonomousProcessManagerAction::SystemPortList => {
                self.process_manager_system_port_list(request)
            }
            AutonomousProcessManagerAction::SystemSignal => {
                self.process_manager_system_signal(request, operator_approved)
            }
            AutonomousProcessManagerAction::SystemKillTree => {
                self.process_manager_system_kill_tree(request, operator_approved)
            }
            AutonomousProcessManagerAction::Signal => Err(CommandError::user_fixable(
                "autonomous_tool_process_manager_signal_unsupported",
                "Xero phase 5 supports external signaling through system_signal; owned generic signal is not implemented yet.",
            )),
        }
    }

    fn process_manager_start(
        &self,
        request: AutonomousProcessManagerRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        self.process_manager_start_like(
            request,
            operator_approved,
            AutonomousProcessManagerAction::Start,
            false,
        )
    }

    fn process_manager_async_start(
        &self,
        request: AutonomousProcessManagerRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        self.process_manager_start_like(
            request,
            operator_approved,
            AutonomousProcessManagerAction::AsyncStart,
            true,
        )
    }

    fn process_manager_start_like(
        &self,
        request: AutonomousProcessManagerRequest,
        operator_approved: bool,
        action: AutonomousProcessManagerAction,
        async_job: bool,
    ) -> CommandResult<AutonomousToolResult> {
        let argv = if request.shell_mode && request.argv.is_empty() {
            default_shell_argv()
        } else {
            request.argv.clone()
        };
        let prepared_request = super::AutonomousCommandRequest {
            argv,
            cwd: request.cwd.clone(),
            timeout_ms: request.timeout_ms,
        };
        let decision =
            self.evaluate_command_policy(self.prepare_command_request(prepared_request)?)?;

        match decision {
            CommandPolicyDecision::Allow { prepared, policy }
                if request.shell_mode && !operator_approved =>
            {
                let policy = shell_mode_requires_operator_policy(policy, &prepared.argv);
                self.unspawned_process_manager_approval_result(request, prepared, policy, action)
            }
            CommandPolicyDecision::Allow { prepared, policy } if request.shell_mode => {
                let policy = operator_approved_shell_policy(policy, &prepared.argv);
                self.spawn_owned_process(
                    request,
                    prepared,
                    process_policy_from_command(policy),
                    SpawnOwnedProcessOptions::new(action).with_async_job(async_job),
                )
            }
            CommandPolicyDecision::Allow { prepared, policy } => self.spawn_owned_process(
                request,
                prepared,
                process_policy_from_command(policy),
                SpawnOwnedProcessOptions::new(action).with_async_job(async_job),
            ),
            CommandPolicyDecision::Escalate { prepared, policy } if operator_approved => {
                let policy = operator_approved_command_policy(policy, &prepared.argv);
                self.spawn_owned_process(
                    request,
                    prepared,
                    process_policy_from_command(policy),
                    SpawnOwnedProcessOptions::new(action).with_async_job(async_job),
                )
            }
            CommandPolicyDecision::Escalate { prepared, policy } => {
                self.unspawned_process_manager_approval_result(request, prepared, policy, action)
            }
        }
    }

    fn spawn_owned_process(
        &self,
        request: AutonomousProcessManagerRequest,
        prepared: PreparedCommandRequest,
        policy: AutonomousProcessManagerPolicyTrace,
        options: SpawnOwnedProcessOptions,
    ) -> CommandResult<AutonomousToolResult> {
        self.owned_processes.ensure_capacity()?;
        self.check_cancelled()?;

        let mut command = Command::new(&prepared.argv[0]);
        let process_type = clean_optional_string(request.process_type.as_deref())
            .or_else(|| options.async_job.then(|| "async_job".to_owned()));
        let launch_config = OwnedProcessLaunchConfig {
            prepared: prepared.clone(),
            shell_mode: request.shell_mode,
            interactive: request.interactive,
            label: clean_optional_string(request.label.as_deref()),
            process_type,
            group: clean_optional_string(request.group.as_deref()),
            persistent: request.persistent,
            async_job: options.async_job,
            timeout_ms: options.async_job.then_some(prepared.timeout_ms),
            output_artifact_dir: project_app_data_dir_for_repo(&self.repo_root)
                .join(ASYNC_JOB_ARTIFACT_DIR),
        };
        let wants_stdin = launch_config.interactive || launch_config.shell_mode;
        command
            .args(prepared.argv.iter().skip(1))
            .current_dir(&prepared.cwd)
            .stdin(if wants_stdin {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_process_tree_root(&mut command);
        apply_sanitized_command_environment(&mut command);

        let mut child = command.spawn().map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => CommandError::user_fixable(
                "autonomous_tool_process_manager_not_found",
                format!("Xero could not find command `{}`.", prepared.argv[0]),
            ),
            _ => CommandError::system_fault(
                "autonomous_tool_process_manager_spawn_failed",
                format!(
                    "Xero could not launch owned process `{}`: {error}",
                    prepared.argv[0]
                ),
            ),
        })?;

        let stdin = if wants_stdin {
            child.stdin.take()
        } else {
            None
        };
        let stdout = child.stdout.take().ok_or_else(|| {
            let _ = terminate_process_tree(&mut child);
            CommandError::system_fault(
                "autonomous_tool_process_manager_stdout_missing",
                "Xero could not capture owned process stdout.",
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            let _ = terminate_process_tree(&mut child);
            CommandError::system_fault(
                "autonomous_tool_process_manager_stderr_missing",
                "Xero could not capture owned process stderr.",
            )
        })?;

        let process_id = options
            .process_id
            .clone()
            .unwrap_or_else(|| self.owned_processes.next_process_id());
        let cwd = display_relative_or_root(&self.repo_root, &prepared.cwd);
        let mut owned_process = OwnedProcess::new(
            process_id.clone(),
            launch_config,
            child,
            stdin,
            options.restart_count,
            options.last_restart_reason.clone(),
        );
        owned_process.set_display_cwd(cwd.clone());
        let process = Arc::new(owned_process);

        spawn_owned_process_reader(
            Arc::clone(&process),
            stdout,
            AutonomousProcessOutputStream::Stdout,
        );
        spawn_owned_process_reader(
            Arc::clone(&process),
            stderr,
            AutonomousProcessOutputStream::Stderr,
        );

        if let Err(error) = self.owned_processes.insert(Arc::clone(&process)) {
            let _ = process.kill();
            return Err(error);
        }

        if options.async_job {
            spawn_async_job_timeout_monitor(
                Arc::clone(&process),
                Duration::from_millis(prepared.timeout_ms),
            );
        }

        thread::sleep(PROCESS_MANAGER_INITIAL_DRAIN);
        if self.is_cancelled() {
            let _ = self.owned_processes.remove(&process_id);
            let _ = process.kill();
            return Err(cancelled_error());
        }

        let exit_code = process.poll_exit()?;
        let chunks = process.read_chunks_after(0, default_process_output_read_bytes())?;
        let metadata = process.metadata()?;
        let running = exit_code.is_none();
        let action_label = process_manager_action_label(options.action);
        let message = match (options.action, running) {
            (AutonomousProcessManagerAction::Restart, true) => {
                format!(
                    "Restarted owned process `{process_id}` for `{}` in `{cwd}`.",
                    render_command_for_summary(&prepared.argv)
                )
            }
            (AutonomousProcessManagerAction::Restart, false) => {
                format!(
                    "Restarted owned process `{process_id}` for `{}` but it exited during startup.",
                    render_command_for_summary(&prepared.argv)
                )
            }
            (AutonomousProcessManagerAction::AsyncStart, true) => {
                format!(
                    "Started async job `{process_id}` for `{}` in `{cwd}` with timeout {} ms.",
                    render_command_for_summary(&prepared.argv),
                    prepared.timeout_ms
                )
            }
            (AutonomousProcessManagerAction::AsyncStart, false) => {
                format!(
                    "Async job `{process_id}` for `{}` exited during startup.",
                    render_command_for_summary(&prepared.argv)
                )
            }
            (_, true) => {
                format!(
                    "Started owned process `{process_id}` for `{}` in `{cwd}`.",
                    render_command_for_summary(&prepared.argv)
                )
            }
            (_, false) => {
                format!(
                    "Owned process `{process_id}` for `{}` exited during {action_label}.",
                    render_command_for_summary(&prepared.argv)
                )
            }
        };

        Ok(process_manager_result(ProcessManagerResultInput {
            action: options.action,
            spawned: true,
            process_id: Some(process_id),
            processes: vec![metadata],
            chunks,
            next_cursor: Some(process.next_cursor_value()),
            policy,
            message,
        }))
    }

    fn unspawned_process_manager_approval_result(
        &self,
        request: AutonomousProcessManagerRequest,
        prepared: PreparedCommandRequest,
        command_policy: AutonomousCommandPolicyTrace,
        action: AutonomousProcessManagerAction,
    ) -> CommandResult<AutonomousToolResult> {
        let cwd = prepared
            .cwd_relative
            .as_ref()
            .map(|path| display_relative_or_root(&self.repo_root, &self.repo_root.join(path)))
            .unwrap_or_else(|| ".".into());
        let policy = process_policy_requiring_command_approval(command_policy);
        let message = format!(
            "Owned process `{}` requires operator review before Xero can start it.",
            render_command_for_summary(&prepared.argv)
        );
        Ok(process_manager_result(ProcessManagerResultInput {
            action,
            spawned: false,
            process_id: Some("unstarted".into()),
            processes: vec![unstarted_process_metadata(
                &prepared.argv,
                cwd,
                request.shell_mode,
                request.label,
                request.process_type,
                request.group,
            )],
            chunks: Vec::new(),
            next_cursor: Some(0),
            policy,
            message,
        }))
    }

    fn process_manager_list(
        &self,
        request: AutonomousProcessManagerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let processes = self.owned_processes.list()?;
        let mut metadata = Vec::with_capacity(processes.len());
        for process in processes {
            let _ = process.poll_exit()?;
            metadata.push(process.metadata()?);
        }
        let message = format!("Listed {} Xero-owned process(es).", metadata.len());
        Ok(process_manager_result(ProcessManagerResultInput {
            action: AutonomousProcessManagerAction::List,
            spawned: true,
            process_id: None,
            processes: metadata,
            chunks: Vec::new(),
            next_cursor: request.after_cursor,
            policy: process_manager_policy_trace(
                AutonomousProcessManagerAction::List,
                request.target_ownership,
                false,
            ),
            message,
        }))
    }

    fn process_manager_status(
        &self,
        request: AutonomousProcessManagerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let process_id = normalized_process_id(&request)?;
        let process = self.owned_processes.get(&process_id)?;
        let _ = process.poll_exit()?;
        let metadata = process.metadata()?;
        let message = format!("Returned status for owned process `{process_id}`.");
        Ok(process_manager_result(ProcessManagerResultInput {
            action: AutonomousProcessManagerAction::Status,
            spawned: true,
            process_id: Some(process_id),
            processes: vec![metadata],
            chunks: Vec::new(),
            next_cursor: request.after_cursor,
            policy: process_manager_policy_trace(
                AutonomousProcessManagerAction::Status,
                request.target_ownership,
                false,
            ),
            message,
        }))
    }

    fn process_manager_output(
        &self,
        request: AutonomousProcessManagerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let process_id = normalized_process_id(&request)?;
        let process = self.owned_processes.get(&process_id)?;
        let _ = process.poll_exit()?;
        let read = read_process_output_for_request(&process, &request)?;
        if let Some(cursor) = read.advance_cursor {
            process.remember_last_read_cursor(cursor);
        }
        let metadata = process.metadata()?;
        let message = format!(
            "Read {} output chunk(s) from owned process `{process_id}`.",
            read.chunks.len()
        );
        Ok(process_manager_result(ProcessManagerResultInput {
            action: AutonomousProcessManagerAction::Output,
            spawned: true,
            process_id: Some(process_id),
            processes: vec![metadata],
            chunks: read.chunks,
            next_cursor: Some(process.next_cursor_value()),
            policy: process_manager_policy_trace(
                AutonomousProcessManagerAction::Output,
                request.target_ownership,
                false,
            ),
            message,
        }))
    }

    fn process_manager_digest(
        &self,
        request: AutonomousProcessManagerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let metadata = self.process_metadata_for_request(&request)?;
        let digest = process_digest(&metadata);
        Ok(process_manager_result(ProcessManagerResultInput {
            action: AutonomousProcessManagerAction::Digest,
            spawned: true,
            process_id: request.process_id.clone(),
            processes: metadata,
            chunks: Vec::new(),
            next_cursor: request.after_cursor,
            policy: process_manager_policy_trace(
                AutonomousProcessManagerAction::Digest,
                request.target_ownership,
                false,
            ),
            message: digest,
        }))
    }

    fn process_manager_highlights(
        &self,
        request: AutonomousProcessManagerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let metadata = self.process_metadata_for_request(&request)?;
        let highlight_count = metadata
            .iter()
            .map(|process| {
                process.detected_urls.len()
                    + process.detected_ports.len()
                    + process.recent_warnings.len()
                    + process.recent_errors.len()
                    + process.recent_stack_traces.len()
                    + process.status_changes.len()
                    + usize::from(process.readiness.ready)
            })
            .sum::<usize>();
        let message = format!(
            "Returned {highlight_count} process highlight(s) from {} Xero-owned process(es).",
            metadata.len()
        );
        Ok(process_manager_result(ProcessManagerResultInput {
            action: AutonomousProcessManagerAction::Highlights,
            spawned: true,
            process_id: request.process_id.clone(),
            processes: metadata,
            chunks: Vec::new(),
            next_cursor: request.after_cursor,
            policy: process_manager_policy_trace(
                AutonomousProcessManagerAction::Highlights,
                request.target_ownership,
                false,
            ),
            message,
        }))
    }

    fn process_manager_wait_for_ready(
        &self,
        request: AutonomousProcessManagerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let process_id = normalized_process_id(&request)?;
        let process = self.owned_processes.get(&process_id)?;
        let timeout = self.process_wait_timeout(request.timeout_ms)?;
        let after_cursor = request.after_cursor.unwrap_or(0);
        let readiness =
            self.wait_for_process_readiness(&process, &request, after_cursor, timeout)?;
        let chunks = process.read_chunks_after(after_cursor, MAX_PROCESS_OUTPUT_READ_BYTES)?;
        let metadata = process.metadata()?;
        let message = if readiness.ready {
            format!(
                "Owned process `{process_id}` is ready via {}.",
                readiness_detector_label(readiness.detector)
            )
        } else {
            format!(
                "Timed out waiting for owned process `{process_id}` readiness via {}.",
                readiness_detector_label(readiness.detector)
            )
        };
        Ok(process_manager_result(ProcessManagerResultInput {
            action: AutonomousProcessManagerAction::WaitForReady,
            spawned: true,
            process_id: Some(process_id),
            processes: vec![metadata],
            chunks,
            next_cursor: Some(process.next_cursor_value()),
            policy: process_manager_policy_trace(
                AutonomousProcessManagerAction::WaitForReady,
                request.target_ownership,
                false,
            ),
            message,
        }))
    }

    fn process_manager_send(
        &self,
        request: AutonomousProcessManagerRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        let process_id = normalized_process_id(&request)?;
        let process = self.owned_processes.get(&process_id)?;
        let input = normalized_stdin_input(&request)?.to_owned();
        if let Some(policy) =
            self.process_shell_input_requires_approval(&process, &input, operator_approved)?
        {
            return self.unperformed_process_interaction_result(
                request,
                process,
                AutonomousProcessManagerAction::Send,
                policy,
                format!("Stdin for owned shell process `{process_id}` requires operator review."),
            );
        }

        let after_cursor = request
            .after_cursor
            .unwrap_or_else(|| process.next_cursor_value().saturating_sub(1));
        process.send_input(&input)?;
        thread::sleep(PROCESS_MANAGER_SEND_DRAIN);
        let _ = process.poll_exit()?;
        let chunks =
            process.read_chunks_after(after_cursor, default_process_output_read_bytes())?;
        let metadata = process.metadata()?;
        Ok(process_manager_result(ProcessManagerResultInput {
            action: AutonomousProcessManagerAction::Send,
            spawned: true,
            process_id: Some(process_id.clone()),
            processes: vec![metadata],
            chunks,
            next_cursor: Some(process.next_cursor_value()),
            policy: process_interaction_policy_allowed(
                AutonomousProcessManagerAction::Send,
                request.target_ownership,
            ),
            message: format!("Wrote stdin to owned process `{process_id}`."),
        }))
    }

    fn process_manager_send_and_wait(
        &self,
        request: AutonomousProcessManagerRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        let process_id = normalized_process_id(&request)?;
        let process = self.owned_processes.get(&process_id)?;
        let input = normalized_stdin_input(&request)?.to_owned();
        if let Some(policy) =
            self.process_shell_input_requires_approval(&process, &input, operator_approved)?
        {
            return self.unperformed_process_interaction_result(
                request,
                process,
                AutonomousProcessManagerAction::SendAndWait,
                policy,
                format!("Stdin for owned shell process `{process_id}` requires operator review."),
            );
        }

        let wait_pattern = request.wait_pattern.as_deref().unwrap_or_default();
        let timeout = self.process_wait_timeout(request.timeout_ms)?;
        let after_cursor = request
            .after_cursor
            .unwrap_or_else(|| process.next_cursor_value().saturating_sub(1));
        process.send_input(&input)?;
        let (chunks, matched) =
            wait_for_output_match(&process, after_cursor, wait_pattern, timeout)?;
        let metadata = process.metadata()?;
        let message = match matched {
            Some(matched) => format!(
                "Wrote stdin to owned process `{process_id}` and observed `{}`.",
                truncate_chars(&matched, 120)
            ),
            None => format!(
                "Wrote stdin to owned process `{process_id}` but timed out waiting for `{wait_pattern}`."
            ),
        };
        Ok(process_manager_result(ProcessManagerResultInput {
            action: AutonomousProcessManagerAction::SendAndWait,
            spawned: true,
            process_id: Some(process_id),
            processes: vec![metadata],
            chunks,
            next_cursor: Some(process.next_cursor_value()),
            policy: process_interaction_policy_allowed(
                AutonomousProcessManagerAction::SendAndWait,
                request.target_ownership,
            ),
            message,
        }))
    }

    fn process_manager_run(
        &self,
        request: AutonomousProcessManagerRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        let process_id = normalized_process_id(&request)?;
        let process = self.owned_processes.get(&process_id)?;
        ensure_shell_process(&process)?;
        let input = normalized_stdin_input(&request)?.to_owned();
        if let Some(policy) =
            self.process_shell_input_requires_approval(&process, &input, operator_approved)?
        {
            return self.unperformed_process_interaction_result(
                request,
                process,
                AutonomousProcessManagerAction::Run,
                policy,
                format!("Shell command for owned process `{process_id}` requires operator review."),
            );
        }

        let timeout = self.process_wait_timeout(request.timeout_ms)?;
        let after_cursor = request
            .after_cursor
            .unwrap_or_else(|| process.next_cursor_value().saturating_sub(1));
        let marker = process_run_marker(&process_id, process.next_cursor_value());
        let payload = shell_run_payload(&input, &marker);
        process.send_input(&payload)?;
        let wait_pattern = format!("{}:-?[0-9]+", regex::escape(&marker));
        let (chunks, matched) =
            wait_for_output_match(&process, after_cursor, &wait_pattern, timeout)?;
        let metadata = process.metadata()?;
        let message = if matched.is_some() {
            format!("Ran a command in owned shell process `{process_id}`.")
        } else {
            format!(
                "Timed out waiting for owned shell process `{process_id}` to finish the command."
            )
        };
        Ok(process_manager_result(ProcessManagerResultInput {
            action: AutonomousProcessManagerAction::Run,
            spawned: true,
            process_id: Some(process_id),
            processes: vec![metadata],
            chunks,
            next_cursor: Some(process.next_cursor_value()),
            policy: process_interaction_policy_allowed(
                AutonomousProcessManagerAction::Run,
                request.target_ownership,
            ),
            message,
        }))
    }

    fn process_manager_env(
        &self,
        request: AutonomousProcessManagerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let process_id = normalized_process_id(&request)?;
        let process = self.owned_processes.get(&process_id)?;
        ensure_shell_process(&process)?;
        let timeout = self.process_wait_timeout(request.timeout_ms)?;
        let after_cursor = request
            .after_cursor
            .unwrap_or_else(|| process.next_cursor_value().saturating_sub(1));
        let marker = process_env_marker(&process_id, process.next_cursor_value());
        let payload = shell_env_payload(&marker);
        process.send_input(&payload)?;
        let wait_pattern = regex::escape(&marker);
        let (chunks, matched) =
            wait_for_output_match(&process, after_cursor, &wait_pattern, timeout)?;
        let metadata = process.metadata()?;
        let message = if matched.is_some() {
            format!("Read environment details from owned shell process `{process_id}`.")
        } else {
            format!(
                "Timed out waiting for environment details from owned shell process `{process_id}`."
            )
        };
        Ok(process_manager_result(ProcessManagerResultInput {
            action: AutonomousProcessManagerAction::Env,
            spawned: true,
            process_id: Some(process_id),
            processes: vec![metadata],
            chunks,
            next_cursor: Some(process.next_cursor_value()),
            policy: process_manager_policy_trace(
                AutonomousProcessManagerAction::Env,
                request.target_ownership,
                false,
            ),
            message,
        }))
    }

    fn process_manager_kill(
        &self,
        request: AutonomousProcessManagerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let process_id = normalized_process_id(&request)?;
        let process = self.owned_processes.remove(&process_id)?;
        let _ = process.kill()?;
        thread::sleep(PROCESS_MANAGER_INITIAL_DRAIN);
        let chunks = process.read_chunks_after(
            request.after_cursor.unwrap_or(0),
            MAX_PROCESS_OUTPUT_READ_BYTES,
        )?;
        let metadata = process.metadata()?;
        let message = format!("Killed owned process `{process_id}`.");
        Ok(process_manager_result(ProcessManagerResultInput {
            action: AutonomousProcessManagerAction::Kill,
            spawned: true,
            process_id: Some(process_id),
            processes: vec![metadata],
            chunks,
            next_cursor: Some(process.next_cursor_value()),
            policy: process_manager_policy_trace(
                AutonomousProcessManagerAction::Kill,
                request.target_ownership,
                false,
            ),
            message,
        }))
    }

    fn process_manager_restart(
        &self,
        request: AutonomousProcessManagerRequest,
        _operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        let process_id = normalized_process_id(&request)?;
        let process = self.owned_processes.remove(&process_id)?;
        let launch_config = process.launch_config();
        let restart_count = process.restart_count_value().saturating_add(1);
        let reason = clean_optional_string(request.input.as_deref())
            .or_else(|| Some("operator_requested".to_owned()));
        let _ = process.kill()?;
        thread::sleep(PROCESS_MANAGER_INITIAL_DRAIN);
        let restart_request = process_manager_request_from_launch_config(
            AutonomousProcessManagerAction::Restart,
            &launch_config,
        );
        let prepared = launch_config.prepared.clone();
        let async_job = launch_config.async_job;

        self.spawn_owned_process(
            restart_request,
            prepared,
            process_manager_policy_trace(
                AutonomousProcessManagerAction::Restart,
                request.target_ownership,
                false,
            ),
            SpawnOwnedProcessOptions::new(AutonomousProcessManagerAction::Restart)
                .with_process_id(process_id)
                .with_restart(restart_count, reason)
                .with_async_job(async_job),
        )
    }

    fn process_manager_group_status(
        &self,
        request: AutonomousProcessManagerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let group = normalized_group(&request)?;
        let metadata = self.process_metadata_for_group(&group)?;
        let digest = if metadata.is_empty() {
            format!("No Xero-owned processes are registered in group `{group}`.")
        } else {
            process_digest(&metadata)
        };
        Ok(process_manager_result(ProcessManagerResultInput {
            action: AutonomousProcessManagerAction::GroupStatus,
            spawned: true,
            process_id: None,
            processes: metadata,
            chunks: Vec::new(),
            next_cursor: request.after_cursor,
            policy: process_manager_policy_trace(
                AutonomousProcessManagerAction::GroupStatus,
                request.target_ownership,
                false,
            ),
            message: digest,
        }))
    }

    fn process_manager_group_kill(
        &self,
        request: AutonomousProcessManagerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let group = normalized_group(&request)?;
        let processes = self.owned_processes.list()?;
        let targets = processes
            .into_iter()
            .filter(|process| process.group.as_deref() == Some(group.as_str()))
            .collect::<Vec<_>>();
        let mut metadata = Vec::with_capacity(targets.len());
        let mut chunks = Vec::new();
        for process in targets {
            let process_id = process.process_id.clone();
            let _ = self.owned_processes.remove(&process_id)?;
            let _ = process.kill()?;
            chunks.extend(process.read_chunks_after(
                request.after_cursor.unwrap_or(0),
                MAX_PROCESS_OUTPUT_READ_BYTES,
            )?);
            metadata.push(process.metadata()?);
        }
        let message = format!(
            "Killed {} Xero-owned process(es) in group `{group}`.",
            metadata.len()
        );
        Ok(process_manager_result(ProcessManagerResultInput {
            action: AutonomousProcessManagerAction::GroupKill,
            spawned: true,
            process_id: None,
            processes: metadata,
            chunks,
            next_cursor: None,
            policy: process_manager_policy_trace(
                AutonomousProcessManagerAction::GroupKill,
                request.target_ownership,
                false,
            ),
            message,
        }))
    }

    fn process_manager_async_await(
        &self,
        request: AutonomousProcessManagerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let timeout = self.process_wait_timeout(request.timeout_ms)?;
        let started = Instant::now();
        let requested_process_id = request
            .process_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);

        loop {
            self.check_cancelled()?;
            let jobs = self.async_jobs_for_await(&request, requested_process_id.as_deref())?;
            if jobs.is_empty() {
                return Ok(process_manager_result(ProcessManagerResultInput {
                    action: AutonomousProcessManagerAction::AsyncAwait,
                    spawned: true,
                    process_id: requested_process_id,
                    processes: Vec::new(),
                    chunks: Vec::new(),
                    next_cursor: request.after_cursor,
                    policy: process_manager_policy_trace(
                        AutonomousProcessManagerAction::AsyncAwait,
                        request.target_ownership,
                        false,
                    ),
                    message: "No Xero-owned async jobs are registered.".into(),
                }));
            }

            for job in &jobs {
                let exit_code = job.poll_exit()?;
                let status = *job.status.lock().map_err(process_status_lock_error)?;
                if exit_code.is_some()
                    || matches!(
                        status,
                        AutonomousProcessStatus::Exited
                            | AutonomousProcessStatus::Failed
                            | AutonomousProcessStatus::Killed
                    )
                {
                    let _ = self.owned_processes.remove(&job.process_id);
                    let chunks = job.read_chunks_after(
                        request.after_cursor.unwrap_or(0),
                        MAX_PROCESS_OUTPUT_READ_BYTES,
                    )?;
                    let metadata = job.metadata()?;
                    let message = format!(
                        "Async job `{}` completed with status {:?} and exit code {:?}.",
                        job.process_id, metadata.status, metadata.exit_code
                    );
                    return Ok(process_manager_result(ProcessManagerResultInput {
                        action: AutonomousProcessManagerAction::AsyncAwait,
                        spawned: true,
                        process_id: Some(job.process_id.clone()),
                        processes: vec![metadata],
                        chunks,
                        next_cursor: Some(job.next_cursor_value()),
                        policy: process_manager_policy_trace(
                            AutonomousProcessManagerAction::AsyncAwait,
                            request.target_ownership,
                            false,
                        ),
                        message,
                    }));
                }
            }

            if started.elapsed() >= timeout {
                let metadata = self.process_metadata_for_jobs(jobs)?;
                return Ok(process_manager_result(ProcessManagerResultInput {
                    action: AutonomousProcessManagerAction::AsyncAwait,
                    spawned: true,
                    process_id: requested_process_id,
                    processes: metadata,
                    chunks: Vec::new(),
                    next_cursor: request.after_cursor,
                    policy: process_manager_policy_trace(
                        AutonomousProcessManagerAction::AsyncAwait,
                        request.target_ownership,
                        false,
                    ),
                    message: "Timed out waiting for Xero-owned async job completion.".into(),
                }));
            }

            thread::sleep(PROCESS_MANAGER_WAIT_POLL);
        }
    }

    fn process_manager_async_cancel(
        &self,
        request: AutonomousProcessManagerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let process_id = normalized_process_id(&request)?;
        let process = self.owned_processes.remove(&process_id)?;
        ensure_async_job(&process)?;
        let _ = process.kill()?;
        thread::sleep(PROCESS_MANAGER_INITIAL_DRAIN);
        let chunks = process.read_chunks_after(
            request.after_cursor.unwrap_or(0),
            MAX_PROCESS_OUTPUT_READ_BYTES,
        )?;
        let metadata = process.metadata()?;
        let message = format!("Cancelled async job `{process_id}`.");
        Ok(process_manager_result(ProcessManagerResultInput {
            action: AutonomousProcessManagerAction::AsyncCancel,
            spawned: true,
            process_id: Some(process_id),
            processes: vec![metadata],
            chunks,
            next_cursor: Some(process.next_cursor_value()),
            policy: process_manager_policy_trace(
                AutonomousProcessManagerAction::AsyncCancel,
                request.target_ownership,
                false,
            ),
            message,
        }))
    }

    fn process_manager_system_process_list(
        &self,
        request: AutonomousProcessManagerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let mut processes = list_system_processes()?;
        let port_filter = request.port.or(request.wait_port);
        let ports = if port_filter.is_some() {
            list_system_ports()?
        } else {
            list_system_ports().unwrap_or_default()
        };
        let port_pids = port_filter.map(|port| {
            ports
                .iter()
                .filter(|entry| entry.local_port == port)
                .filter_map(|entry| entry.pid)
                .collect::<BTreeSet<_>>()
        });
        filter_system_processes(&mut processes, &request, port_pids.as_ref())?;
        let total = processes.len();
        processes.sort_by_key(|process| process.pid);
        processes.truncate(MAX_SYSTEM_PROCESS_RESULTS);

        let owned_pids = self.owned_process_pid_set()?;
        let ports_by_pid = system_ports_by_pid(ports);
        let metadata = processes
            .iter()
            .map(|process| system_process_metadata(process, &owned_pids, &ports_by_pid))
            .collect::<Vec<_>>();
        let truncated = total > metadata.len();
        let message = if truncated {
            format!(
                "Listed {} external/system process(es), truncated from {total}.",
                metadata.len()
            )
        } else {
            format!("Listed {} external/system process(es).", metadata.len())
        };

        Ok(process_manager_result(ProcessManagerResultInput {
            action: AutonomousProcessManagerAction::SystemProcessList,
            spawned: true,
            process_id: request.process_id.clone(),
            processes: metadata,
            chunks: Vec::new(),
            next_cursor: request.after_cursor,
            policy: process_manager_policy_trace(
                AutonomousProcessManagerAction::SystemProcessList,
                Some(AutonomousProcessOwnershipScope::External),
                false,
            ),
            message,
        }))
    }

    fn process_manager_system_process_tree(
        &self,
        request: AutonomousProcessManagerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let pid = normalized_system_pid(&request)?;
        let processes = list_system_processes()?;
        let tree = system_process_tree(&processes, pid)?;
        let owned_pids = self.owned_process_pid_set()?;
        let ports_by_pid = system_ports_by_pid(list_system_ports().unwrap_or_default());
        let metadata = tree
            .iter()
            .map(|process| system_process_metadata(process, &owned_pids, &ports_by_pid))
            .collect::<Vec<_>>();
        let message = format!(
            "Inspected process tree for external/system PID {pid} with {} related process(es).",
            metadata.len()
        );

        Ok(process_manager_result(ProcessManagerResultInput {
            action: AutonomousProcessManagerAction::SystemProcessTree,
            spawned: true,
            process_id: Some(pid.to_string()),
            processes: metadata,
            chunks: Vec::new(),
            next_cursor: request.after_cursor,
            policy: process_manager_policy_trace(
                AutonomousProcessManagerAction::SystemProcessTree,
                Some(AutonomousProcessOwnershipScope::External),
                false,
            ),
            message,
        }))
    }

    fn process_manager_system_port_list(
        &self,
        request: AutonomousProcessManagerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let mut ports = list_system_ports()?;
        filter_system_ports(&mut ports, &request)?;
        let total = ports.len();
        ports.sort_by(|left, right| {
            left.local_port
                .cmp(&right.local_port)
                .then_with(|| left.local_addr.cmp(&right.local_addr))
                .then_with(|| left.pid.cmp(&right.pid))
        });
        ports.truncate(MAX_SYSTEM_PORT_RESULTS);

        let system_processes = list_system_processes().unwrap_or_default();
        let process_by_pid = system_processes
            .iter()
            .map(|process| (process.pid, process.clone()))
            .collect::<BTreeMap<_, _>>();
        let owned_pids = self.owned_process_pid_set()?;
        let ports_by_pid = system_ports_by_pid(ports.clone());
        let mut seen_pids = BTreeSet::new();
        let metadata = ports
            .iter()
            .filter_map(|port| port.pid)
            .filter(|pid| seen_pids.insert(*pid))
            .filter_map(|pid| process_by_pid.get(&pid))
            .map(|process| system_process_metadata(process, &owned_pids, &ports_by_pid))
            .collect::<Vec<_>>();
        let output_ports = ports.iter().map(system_port_output).collect::<Vec<_>>();
        let truncated = total > output_ports.len();
        let message = if truncated {
            format!(
                "Listed {} local listening port(s), truncated from {total}.",
                output_ports.len()
            )
        } else {
            format!("Listed {} local listening port(s).", output_ports.len())
        };

        Ok(process_manager_result_with_ports(
            ProcessManagerResultInput {
                action: AutonomousProcessManagerAction::SystemPortList,
                spawned: true,
                process_id: request.process_id.clone(),
                processes: metadata,
                chunks: Vec::new(),
                next_cursor: request.after_cursor,
                policy: process_manager_policy_trace(
                    AutonomousProcessManagerAction::SystemPortList,
                    Some(AutonomousProcessOwnershipScope::External),
                    false,
                ),
                message,
            },
            output_ports,
        ))
    }

    fn process_manager_system_signal(
        &self,
        request: AutonomousProcessManagerRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        let pid = normalized_system_pid(&request)?;
        let signal = normalized_external_signal(request.signal.as_deref())?;
        let processes = list_system_processes()?;
        let target = system_process_by_pid(&processes, pid)?;
        let owned_pids = self.owned_process_pid_set()?;
        ensure_external_signal_target(pid, &processes, &owned_pids, false)?;
        let ports_by_pid = system_ports_by_pid(list_system_ports().unwrap_or_default());
        let metadata = vec![system_process_metadata(&target, &owned_pids, &ports_by_pid)];
        let approval_policy = process_manager_policy_trace(
            AutonomousProcessManagerAction::SystemSignal,
            Some(AutonomousProcessOwnershipScope::External),
            false,
        );
        if !operator_approved {
            return Ok(process_manager_result(ProcessManagerResultInput {
                action: AutonomousProcessManagerAction::SystemSignal,
                spawned: false,
                process_id: Some(pid.to_string()),
                processes: metadata,
                chunks: Vec::new(),
                next_cursor: request.after_cursor,
                policy: approval_policy,
                message: format!(
                    "External process PID {pid} requires operator review before Xero can send signal {}.",
                    signal.label
                ),
            }));
        }

        signal_external_pid(pid, signal.number)?;
        let mut metadata = metadata;
        if let Some(process) = metadata.first_mut() {
            process.status_changes.push(format!(
                "sent signal {} to external PID {pid}",
                signal.label
            ));
        }
        Ok(process_manager_result(ProcessManagerResultInput {
            action: AutonomousProcessManagerAction::SystemSignal,
            spawned: true,
            process_id: Some(pid.to_string()),
            processes: metadata,
            chunks: Vec::new(),
            next_cursor: request.after_cursor,
            policy: external_signal_policy_allowed(
                AutonomousProcessManagerAction::SystemSignal,
                signal.label,
            ),
            message: format!(
                "Sent signal {} to external process PID {pid}.",
                signal.label
            ),
        }))
    }

    fn process_manager_system_kill_tree(
        &self,
        request: AutonomousProcessManagerRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        let pid = normalized_system_pid(&request)?;
        let processes = list_system_processes()?;
        let tree = system_process_tree(&processes, pid)?;
        let owned_pids = self.owned_process_pid_set()?;
        ensure_external_signal_target(pid, &processes, &owned_pids, true)?;
        let ports_by_pid = system_ports_by_pid(list_system_ports().unwrap_or_default());
        let metadata = tree
            .iter()
            .map(|process| system_process_metadata(process, &owned_pids, &ports_by_pid))
            .collect::<Vec<_>>();
        let approval_policy = process_manager_policy_trace(
            AutonomousProcessManagerAction::SystemKillTree,
            Some(AutonomousProcessOwnershipScope::External),
            false,
        );
        if !operator_approved {
            return Ok(process_manager_result(ProcessManagerResultInput {
                action: AutonomousProcessManagerAction::SystemKillTree,
                spawned: false,
                process_id: Some(pid.to_string()),
                processes: metadata,
                chunks: Vec::new(),
                next_cursor: request.after_cursor,
                policy: approval_policy,
                message: format!(
                    "External process tree rooted at PID {pid} requires operator review before Xero can kill it."
                ),
            }));
        }

        let killed_pids = kill_external_process_tree(&tree, pid)?;
        let mut metadata = metadata;
        for process in &mut metadata {
            if process.pid.is_some_and(|pid| killed_pids.contains(&pid)) {
                process.status = if process_exists(process.pid.unwrap_or_default()) {
                    AutonomousProcessStatus::Unknown
                } else {
                    AutonomousProcessStatus::Killed
                };
                process
                    .status_changes
                    .push(format!("attempted external tree kill for PID {pid}"));
            }
        }
        Ok(process_manager_result(ProcessManagerResultInput {
            action: AutonomousProcessManagerAction::SystemKillTree,
            spawned: true,
            process_id: Some(pid.to_string()),
            processes: metadata,
            chunks: Vec::new(),
            next_cursor: request.after_cursor,
            policy: external_signal_policy_allowed(
                AutonomousProcessManagerAction::SystemKillTree,
                "TERM/KILL",
            ),
            message: format!(
                "Killed external process tree rooted at PID {pid} ({} target process(es)).",
                killed_pids.len()
            ),
        }))
    }

    fn unperformed_process_interaction_result(
        &self,
        request: AutonomousProcessManagerRequest,
        process: Arc<OwnedProcess>,
        action: AutonomousProcessManagerAction,
        policy: AutonomousProcessManagerPolicyTrace,
        message: String,
    ) -> CommandResult<AutonomousToolResult> {
        let _ = process.poll_exit()?;
        let metadata = process.metadata()?;
        Ok(process_manager_result(ProcessManagerResultInput {
            action,
            spawned: false,
            process_id: Some(process.process_id.clone()),
            processes: vec![metadata],
            chunks: Vec::new(),
            next_cursor: Some(process.next_cursor_value()),
            policy,
            message: if request.input.is_some() {
                message
            } else {
                format!("{message} No stdin payload was written.")
            },
        }))
    }

    fn process_shell_input_requires_approval(
        &self,
        process: &OwnedProcess,
        input: &str,
        operator_approved: bool,
    ) -> CommandResult<Option<AutonomousProcessManagerPolicyTrace>> {
        if !process.command.shell_mode {
            return Ok(None);
        }

        let request = super::AutonomousCommandRequest {
            argv: shell_policy_argv(input),
            cwd: None,
            timeout_ms: None,
        };
        let decision = self.evaluate_command_policy(self.prepare_command_request(request)?)?;
        match decision {
            CommandPolicyDecision::Allow { .. } => Ok(None),
            CommandPolicyDecision::Escalate { prepared, policy } if operator_approved => {
                let _ = operator_approved_command_policy(policy, &prepared.argv);
                Ok(None)
            }
            CommandPolicyDecision::Escalate { policy, .. } => {
                Ok(Some(process_policy_requiring_command_approval(policy)))
            }
        }
    }

    fn process_wait_timeout(&self, timeout_ms: Option<u64>) -> CommandResult<Duration> {
        let timeout = timeout_ms.unwrap_or(self.limits.default_command_timeout_ms);
        if timeout == 0 || timeout > self.limits.max_command_timeout_ms {
            return Err(CommandError::user_fixable(
                "autonomous_tool_process_manager_timeout_invalid",
                format!(
                    "Xero requires process_manager timeoutMs to be between 1 and {}.",
                    self.limits.max_command_timeout_ms
                ),
            ));
        }
        Ok(Duration::from_millis(timeout))
    }

    pub(crate) fn owned_process_lifecycle_summary(&self) -> CommandResult<Option<String>> {
        let metadata = self.process_metadata_for_request(&AutonomousProcessManagerRequest {
            action: AutonomousProcessManagerAction::Digest,
            process_id: None,
            pid: None,
            parent_pid: None,
            port: None,
            group: None,
            label: None,
            process_type: None,
            argv: Vec::new(),
            cwd: None,
            shell_mode: false,
            interactive: false,
            target_ownership: None,
            persistent: false,
            timeout_ms: None,
            after_cursor: None,
            since_last_read: false,
            max_bytes: None,
            tail_lines: None,
            stream: None,
            filter: None,
            input: None,
            wait_pattern: None,
            wait_port: None,
            wait_url: None,
            signal: None,
        })?;
        if metadata.is_empty() {
            Ok(None)
        } else {
            Ok(Some(process_digest(&metadata)))
        }
    }

    fn owned_process_pid_set(&self) -> CommandResult<BTreeSet<u32>> {
        Ok(self
            .owned_processes
            .list()?
            .into_iter()
            .map(|process| process.pid)
            .collect())
    }
}

struct ProcessManagerResultInput {
    action: AutonomousProcessManagerAction,
    spawned: bool,
    process_id: Option<String>,
    processes: Vec<AutonomousProcessMetadata>,
    chunks: Vec<AutonomousProcessOutputChunk>,
    next_cursor: Option<u64>,
    policy: AutonomousProcessManagerPolicyTrace,
    message: String,
}

fn process_manager_result(input: ProcessManagerResultInput) -> AutonomousToolResult {
    process_manager_result_with_ports(input, Vec::new())
}

fn process_manager_result_with_ports(
    input: ProcessManagerResultInput,
    system_ports: Vec<AutonomousSystemPort>,
) -> AutonomousToolResult {
    let digest = if matches!(
        input.action,
        AutonomousProcessManagerAction::Digest | AutonomousProcessManagerAction::GroupStatus
    ) {
        Some(input.message.clone())
    } else {
        None
    };
    let highlights = result_highlights(input.action, &input.processes, &input.chunks);
    AutonomousToolResult {
        tool_name: AUTONOMOUS_TOOL_PROCESS_MANAGER.into(),
        summary: input.message.clone(),
        command_result: None,
        output: AutonomousToolOutput::ProcessManager(AutonomousProcessManagerOutput {
            action: input.action,
            phase: PROCESS_MANAGER_PHASE.into(),
            spawned: input.spawned,
            process_id: input.process_id,
            processes: input.processes,
            system_ports,
            chunks: input.chunks,
            next_cursor: input.next_cursor,
            digest,
            highlights,
            policy: input.policy,
            contract: process_manager_contract(),
            message: input.message,
        }),
    }
}

impl AutonomousToolRuntime {
    fn process_metadata_for_request(
        &self,
        request: &AutonomousProcessManagerRequest,
    ) -> CommandResult<Vec<AutonomousProcessMetadata>> {
        let processes = if let Some(process_id) = request
            .process_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            vec![self.owned_processes.get(process_id)?]
        } else {
            self.owned_processes.list()?
        };

        let mut metadata = Vec::with_capacity(processes.len());
        for process in processes {
            let _ = process.poll_exit()?;
            metadata.push(process.metadata()?);
        }
        Ok(metadata)
    }

    fn process_metadata_for_group(
        &self,
        group: &str,
    ) -> CommandResult<Vec<AutonomousProcessMetadata>> {
        let processes = self.owned_processes.list()?;
        let targets = processes
            .into_iter()
            .filter(|process| process.group.as_deref() == Some(group))
            .collect::<Vec<_>>();
        self.process_metadata_for_jobs(targets)
    }

    fn process_metadata_for_jobs(
        &self,
        processes: Vec<Arc<OwnedProcess>>,
    ) -> CommandResult<Vec<AutonomousProcessMetadata>> {
        let mut metadata = Vec::with_capacity(processes.len());
        for process in processes {
            let _ = process.poll_exit()?;
            metadata.push(process.metadata()?);
        }
        Ok(metadata)
    }

    fn async_jobs_for_await(
        &self,
        request: &AutonomousProcessManagerRequest,
        process_id: Option<&str>,
    ) -> CommandResult<Vec<Arc<OwnedProcess>>> {
        if let Some(process_id) = process_id {
            let process = self.owned_processes.get(process_id)?;
            ensure_async_job(&process)?;
            return Ok(vec![process]);
        }

        let group = request
            .group
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let jobs = self
            .owned_processes
            .list()?
            .into_iter()
            .filter(|process| process.is_async_job())
            .filter(|process| group.is_none_or(|group| process.group.as_deref() == Some(group)))
            .collect::<Vec<_>>();
        Ok(jobs)
    }

    fn wait_for_process_readiness(
        &self,
        process: &OwnedProcess,
        request: &AutonomousProcessManagerRequest,
        after_cursor: u64,
        timeout: Duration,
    ) -> CommandResult<AutonomousProcessReadinessState> {
        if let Some(pattern) = request.wait_pattern.as_deref() {
            let (_chunks, matched) =
                wait_for_output_match(process, after_cursor, pattern, timeout)?;
            if let Some(matched) = matched {
                process.mark_ready(
                    AutonomousProcessReadinessDetector::OutputRegex,
                    matched.clone(),
                )?;
                return Ok(AutonomousProcessReadinessState {
                    ready: true,
                    detector: Some(AutonomousProcessReadinessDetector::OutputRegex),
                    matched: Some(matched),
                });
            }
            return Ok(AutonomousProcessReadinessState {
                ready: false,
                detector: Some(AutonomousProcessReadinessDetector::OutputRegex),
                matched: None,
            });
        }

        if let Some(url) = request.wait_url.as_deref() {
            let parsed = parse_local_http_url(url)?;
            return self.wait_for_http_readiness(process, parsed, timeout);
        }

        if let Some(port) = request.wait_port {
            return self.wait_for_port_readiness(process, port, timeout);
        }

        self.wait_for_process_exit_readiness(process, timeout)
    }

    fn wait_for_port_readiness(
        &self,
        process: &OwnedProcess,
        port: u16,
        timeout: Duration,
    ) -> CommandResult<AutonomousProcessReadinessState> {
        let started = Instant::now();
        loop {
            self.check_cancelled()?;
            if port_is_open(port) {
                let matched = format!("localhost:{port}");
                process.mark_ready(
                    AutonomousProcessReadinessDetector::PortOpen,
                    matched.clone(),
                )?;
                return Ok(AutonomousProcessReadinessState {
                    ready: true,
                    detector: Some(AutonomousProcessReadinessDetector::PortOpen),
                    matched: Some(matched),
                });
            }
            if process.poll_exit()?.is_some() || started.elapsed() >= timeout {
                return Ok(AutonomousProcessReadinessState {
                    ready: false,
                    detector: Some(AutonomousProcessReadinessDetector::PortOpen),
                    matched: None,
                });
            }
            thread::sleep(PROCESS_MANAGER_WAIT_POLL);
        }
    }

    fn wait_for_http_readiness(
        &self,
        process: &OwnedProcess,
        url: Url,
        timeout: Duration,
    ) -> CommandResult<AutonomousProcessReadinessState> {
        let client = reqwest::blocking::Client::builder()
            .timeout(PROCESS_MANAGER_HTTP_PROBE_TIMEOUT)
            .build()
            .map_err(|error| {
                CommandError::system_fault(
                    "autonomous_tool_process_manager_http_client_failed",
                    format!("Xero could not create a readiness HTTP client: {error}"),
                )
            })?;
        let started = Instant::now();
        loop {
            self.check_cancelled()?;
            if http_url_is_ready(&client, url.clone()) {
                let matched = url.to_string();
                process.mark_ready(AutonomousProcessReadinessDetector::HttpUrl, matched.clone())?;
                return Ok(AutonomousProcessReadinessState {
                    ready: true,
                    detector: Some(AutonomousProcessReadinessDetector::HttpUrl),
                    matched: Some(matched),
                });
            }
            if process.poll_exit()?.is_some() || started.elapsed() >= timeout {
                return Ok(AutonomousProcessReadinessState {
                    ready: false,
                    detector: Some(AutonomousProcessReadinessDetector::HttpUrl),
                    matched: None,
                });
            }
            thread::sleep(PROCESS_MANAGER_WAIT_POLL);
        }
    }

    fn wait_for_process_exit_readiness(
        &self,
        process: &OwnedProcess,
        timeout: Duration,
    ) -> CommandResult<AutonomousProcessReadinessState> {
        let started = Instant::now();
        loop {
            self.check_cancelled()?;
            if let Some(exit_code) = process.poll_exit()? {
                let matched = format!("exit_code={exit_code}");
                process.mark_ready(
                    AutonomousProcessReadinessDetector::ProcessExit,
                    matched.clone(),
                )?;
                return Ok(AutonomousProcessReadinessState {
                    ready: true,
                    detector: Some(AutonomousProcessReadinessDetector::ProcessExit),
                    matched: Some(matched),
                });
            }
            if started.elapsed() >= timeout {
                return Ok(AutonomousProcessReadinessState {
                    ready: false,
                    detector: Some(AutonomousProcessReadinessDetector::ProcessExit),
                    matched: None,
                });
            }
            thread::sleep(PROCESS_MANAGER_WAIT_POLL);
        }
    }
}

fn validate_process_manager_request(
    request: &AutonomousProcessManagerRequest,
) -> CommandResult<()> {
    match request.action {
        AutonomousProcessManagerAction::Start | AutonomousProcessManagerAction::AsyncStart => {
            if !request.shell_mode && (request.argv.is_empty() || request.argv[0].trim().is_empty())
            {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_process_manager_start_invalid",
                    "Xero requires process_manager start requests to include a non-empty argv[0].",
                ));
            }
            if !request.argv.is_empty() {
                validate_argv_contract(&request.argv)?;
            }
        }
        AutonomousProcessManagerAction::Status
        | AutonomousProcessManagerAction::Output
        | AutonomousProcessManagerAction::WaitForReady
        | AutonomousProcessManagerAction::Env
        | AutonomousProcessManagerAction::Signal
        | AutonomousProcessManagerAction::Kill
        | AutonomousProcessManagerAction::Restart
        | AutonomousProcessManagerAction::AsyncCancel => {
            validate_non_empty(
                request.process_id.as_deref().unwrap_or_default(),
                "processId",
            )?;
        }
        AutonomousProcessManagerAction::Digest
        | AutonomousProcessManagerAction::Highlights
        | AutonomousProcessManagerAction::AsyncAwait
        | AutonomousProcessManagerAction::SystemProcessList
        | AutonomousProcessManagerAction::SystemPortList => {}
        AutonomousProcessManagerAction::SystemProcessTree
        | AutonomousProcessManagerAction::SystemSignal
        | AutonomousProcessManagerAction::SystemKillTree => {
            if request.pid.is_none()
                && parse_process_id_pid(request.process_id.as_deref()).is_none()
            {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_process_manager_pid_required",
                    "Xero requires system process actions to include pid or a numeric processId.",
                ));
            }
        }
        AutonomousProcessManagerAction::Send
        | AutonomousProcessManagerAction::SendAndWait
        | AutonomousProcessManagerAction::Run => {
            validate_non_empty(
                request.process_id.as_deref().unwrap_or_default(),
                "processId",
            )?;
            validate_non_empty(request.input.as_deref().unwrap_or_default(), "input")?;
        }
        AutonomousProcessManagerAction::GroupStatus | AutonomousProcessManagerAction::GroupKill => {
            validate_non_empty(request.group.as_deref().unwrap_or_default(), "group")?;
        }
        AutonomousProcessManagerAction::List => {}
    }

    if let Some(cwd) = request.cwd.as_deref() {
        normalize_relative_path(cwd, "cwd")?;
    }
    if let Some(label) = request.label.as_deref() {
        validate_non_empty(label, "label")?;
    }
    if let Some(process_type) = request.process_type.as_deref() {
        validate_non_empty(process_type, "processType")?;
    }
    if let Some(signal) = request.signal.as_deref() {
        validate_non_empty(signal, "signal")?;
    }
    if let Some(wait_pattern) = request.wait_pattern.as_deref() {
        validate_non_empty(wait_pattern, "waitPattern")?;
    }
    if let Some(filter) = request.filter.as_deref() {
        validate_non_empty(filter, "filter")?;
    }
    if let Some(tail_lines) = request.tail_lines {
        if tail_lines == 0 || tail_lines > MAX_PROCESS_OUTPUT_TAIL_LINES {
            return Err(CommandError::user_fixable(
                "autonomous_tool_process_manager_tail_lines_invalid",
                format!(
                    "Xero requires process_manager tailLines to be between 1 and {MAX_PROCESS_OUTPUT_TAIL_LINES}."
                ),
            ));
        }
    }
    if request.action == AutonomousProcessManagerAction::SendAndWait
        && request
            .wait_pattern
            .as_deref()
            .unwrap_or_default()
            .trim()
            .is_empty()
    {
        return Err(CommandError::user_fixable(
            "autonomous_tool_process_manager_wait_pattern_required",
            "Xero requires send_and_wait requests to include waitPattern.",
        ));
    }
    if request.action == AutonomousProcessManagerAction::AsyncStart && request.timeout_ms == Some(0)
    {
        return Err(CommandError::user_fixable(
            "autonomous_tool_process_manager_timeout_invalid",
            "Xero requires async_start timeoutMs to be greater than zero when provided.",
        ));
    }
    if let Some(wait_url) = request.wait_url.as_deref() {
        validate_non_empty(wait_url, "waitUrl")?;
    }

    Ok(())
}

fn validate_phase_5_scope(request: &AutonomousProcessManagerRequest) -> CommandResult<()> {
    let is_system_action = matches!(
        request.action,
        AutonomousProcessManagerAction::SystemProcessList
            | AutonomousProcessManagerAction::SystemProcessTree
            | AutonomousProcessManagerAction::SystemPortList
            | AutonomousProcessManagerAction::SystemSignal
            | AutonomousProcessManagerAction::SystemKillTree
    );
    if !is_system_action
        && request.target_ownership == Some(AutonomousProcessOwnershipScope::External)
    {
        return Err(CommandError::user_fixable(
            "autonomous_tool_process_manager_external_unsupported",
            "Xero process_manager external ownership is only supported by the phase 5 system_* actions.",
        ));
    }
    if request.persistent {
        return Err(CommandError::user_fixable(
            "autonomous_tool_process_manager_persistent_unsupported",
            "Xero phase 5 process_manager does not support durable background persistence yet.",
        ));
    }
    match request.action {
        AutonomousProcessManagerAction::Start
        | AutonomousProcessManagerAction::AsyncStart
        | AutonomousProcessManagerAction::List
        | AutonomousProcessManagerAction::Status
        | AutonomousProcessManagerAction::Output
        | AutonomousProcessManagerAction::Digest
        | AutonomousProcessManagerAction::WaitForReady
        | AutonomousProcessManagerAction::Highlights
        | AutonomousProcessManagerAction::Send
        | AutonomousProcessManagerAction::SendAndWait
        | AutonomousProcessManagerAction::Run
        | AutonomousProcessManagerAction::Env
        | AutonomousProcessManagerAction::Kill
        | AutonomousProcessManagerAction::Restart
        | AutonomousProcessManagerAction::GroupStatus
        | AutonomousProcessManagerAction::GroupKill
        | AutonomousProcessManagerAction::AsyncAwait
        | AutonomousProcessManagerAction::AsyncCancel
        | AutonomousProcessManagerAction::SystemProcessList
        | AutonomousProcessManagerAction::SystemProcessTree
        | AutonomousProcessManagerAction::SystemPortList
        | AutonomousProcessManagerAction::SystemSignal
        | AutonomousProcessManagerAction::SystemKillTree => Ok(()),
        AutonomousProcessManagerAction::Signal => Err(CommandError::user_fixable(
            "autonomous_tool_process_manager_signal_unsupported",
            "Xero phase 5 supports external signaling through system_signal; owned generic signal is not implemented yet.",
        )),
    }
}

fn validate_argv_contract(argv: &[String]) -> CommandResult<()> {
    if argv.iter().any(|argument| argument.contains('\0')) {
        return Err(CommandError::user_fixable(
            "autonomous_tool_process_manager_start_invalid",
            "Xero refused a process_manager command that contained a NUL byte.",
        ));
    }

    let _redacted = redact_command_argv_for_persistence(argv);
    let mut probe = Command::new(&argv[0]);
    apply_sanitized_command_environment(&mut probe);
    Ok(())
}

pub(super) fn process_manager_contract() -> AutonomousProcessManagerContract {
    AutonomousProcessManagerContract {
        phase: PROCESS_MANAGER_PHASE.into(),
        supported_actions: vec![
            AutonomousProcessManagerAction::Start,
            AutonomousProcessManagerAction::AsyncStart,
            AutonomousProcessManagerAction::List,
            AutonomousProcessManagerAction::Status,
            AutonomousProcessManagerAction::Output,
            AutonomousProcessManagerAction::Digest,
            AutonomousProcessManagerAction::WaitForReady,
            AutonomousProcessManagerAction::Highlights,
            AutonomousProcessManagerAction::Send,
            AutonomousProcessManagerAction::SendAndWait,
            AutonomousProcessManagerAction::Run,
            AutonomousProcessManagerAction::Env,
            AutonomousProcessManagerAction::Kill,
            AutonomousProcessManagerAction::Restart,
            AutonomousProcessManagerAction::GroupStatus,
            AutonomousProcessManagerAction::GroupKill,
            AutonomousProcessManagerAction::AsyncAwait,
            AutonomousProcessManagerAction::AsyncCancel,
            AutonomousProcessManagerAction::SystemProcessList,
            AutonomousProcessManagerAction::SystemProcessTree,
            AutonomousProcessManagerAction::SystemPortList,
            AutonomousProcessManagerAction::SystemSignal,
            AutonomousProcessManagerAction::SystemKillTree,
        ],
        ownership_fields: vec![
            "threadId".into(),
            "sessionId".into(),
            "repoId".into(),
            "userId".into(),
            "scope".into(),
        ],
        risk_levels: vec![
            AutonomousProcessActionRiskLevel::Observe,
            AutonomousProcessActionRiskLevel::RunOwned,
            AutonomousProcessActionRiskLevel::SignalOwned,
            AutonomousProcessActionRiskLevel::SignalExternal,
            AutonomousProcessActionRiskLevel::PersistentBackground,
            AutonomousProcessActionRiskLevel::SystemRead,
            AutonomousProcessActionRiskLevel::OsAutomation,
        ],
        output_limits: AutonomousProcessOutputLimits {
            recent_output_ring_bytes: RECENT_OUTPUT_RING_BYTES,
            recent_output_ring_chunks: RECENT_OUTPUT_RING_CHUNKS,
            full_output_artifact_threshold_bytes: FULL_OUTPUT_ARTIFACT_THRESHOLD_BYTES,
            excerpt_bytes: PROCESS_OUTPUT_EXCERPT_BYTES,
            cursor_kind: "monotonic_output_cursor".into(),
        },
        persistence: AutonomousProcessPersistenceContract {
            persist_metadata: true,
            persist_output_chunks: true,
            redact_before_persistence: true,
            persist_policy_trace: true,
            full_output_artifacts: true,
        },
        lifecycle: AutonomousProcessLifecycleContract {
            app_shutdown: "terminate_non_persistent_xero_owned_process_trees".into(),
            thread_switch: "reinject_owned_process_digest_without_granting_new_control".into(),
            session_compaction: "persist_metadata_and_reinject_digest_with_output_cursors".into(),
            crash_recovery: "owned_processes_marked_unknown_until_reobserved".into(),
        },
    }
}

fn process_manager_action_label(action: AutonomousProcessManagerAction) -> &'static str {
    match action {
        AutonomousProcessManagerAction::Start => "start",
        AutonomousProcessManagerAction::List => "list",
        AutonomousProcessManagerAction::Status => "status",
        AutonomousProcessManagerAction::Output => "output",
        AutonomousProcessManagerAction::Digest => "digest",
        AutonomousProcessManagerAction::WaitForReady => "wait_for_ready",
        AutonomousProcessManagerAction::Highlights => "highlights",
        AutonomousProcessManagerAction::Send => "send",
        AutonomousProcessManagerAction::SendAndWait => "send_and_wait",
        AutonomousProcessManagerAction::Run => "run",
        AutonomousProcessManagerAction::Env => "env",
        AutonomousProcessManagerAction::Signal => "signal",
        AutonomousProcessManagerAction::Kill => "kill",
        AutonomousProcessManagerAction::Restart => "restart",
        AutonomousProcessManagerAction::GroupStatus => "group_status",
        AutonomousProcessManagerAction::GroupKill => "group_kill",
        AutonomousProcessManagerAction::AsyncStart => "async_start",
        AutonomousProcessManagerAction::AsyncAwait => "async_await",
        AutonomousProcessManagerAction::AsyncCancel => "async_cancel",
        AutonomousProcessManagerAction::SystemProcessList => "system_process_list",
        AutonomousProcessManagerAction::SystemProcessTree => "system_process_tree",
        AutonomousProcessManagerAction::SystemPortList => "system_port_list",
        AutonomousProcessManagerAction::SystemSignal => "system_signal",
        AutonomousProcessManagerAction::SystemKillTree => "system_kill_tree",
    }
}

fn normalized_process_id(request: &AutonomousProcessManagerRequest) -> CommandResult<String> {
    let process_id = request.process_id.as_deref().unwrap_or_default().trim();
    validate_non_empty(process_id, "processId")?;
    Ok(process_id.to_owned())
}

fn normalized_group(request: &AutonomousProcessManagerRequest) -> CommandResult<String> {
    let group = request.group.as_deref().unwrap_or_default().trim();
    validate_non_empty(group, "group")?;
    Ok(group.to_owned())
}

fn normalized_stdin_input(request: &AutonomousProcessManagerRequest) -> CommandResult<&str> {
    let input = request.input.as_deref().unwrap_or_default();
    validate_non_empty(input, "input")?;
    if input.contains('\0') {
        return Err(CommandError::user_fixable(
            "autonomous_tool_process_manager_input_invalid",
            "Xero refused a process_manager stdin payload that contained a NUL byte.",
        ));
    }
    if input.len() > MAX_PROCESS_STDIN_INPUT_BYTES {
        return Err(CommandError::user_fixable(
            "autonomous_tool_process_manager_input_too_large",
            format!(
                "Xero limits process_manager stdin payloads to {MAX_PROCESS_STDIN_INPUT_BYTES} bytes."
            ),
        ));
    }
    Ok(input)
}

fn ensure_shell_process(process: &OwnedProcess) -> CommandResult<()> {
    if process.command.shell_mode {
        return Ok(());
    }

    Err(CommandError::user_fixable(
        "autonomous_tool_process_manager_shell_required",
        format!(
            "Xero can only use this action with a shell-mode owned process; `{}` was started as argv mode.",
            process.process_id
        ),
    ))
}

fn ensure_async_job(process: &OwnedProcess) -> CommandResult<()> {
    if process.is_async_job() {
        return Ok(());
    }

    Err(CommandError::user_fixable(
        "autonomous_tool_process_manager_async_job_required",
        format!(
            "Xero can only use this action with an async job; `{}` is a managed process.",
            process.process_id
        ),
    ))
}

fn process_manager_request_from_launch_config(
    action: AutonomousProcessManagerAction,
    launch_config: &OwnedProcessLaunchConfig,
) -> AutonomousProcessManagerRequest {
    AutonomousProcessManagerRequest {
        action,
        process_id: None,
        pid: None,
        parent_pid: None,
        port: None,
        group: launch_config.group.clone(),
        label: launch_config.label.clone(),
        process_type: launch_config.process_type.clone(),
        argv: launch_config.prepared.argv.clone(),
        cwd: launch_config
            .prepared
            .cwd_relative
            .as_ref()
            .map(|path| path.to_string_lossy().replace('\\', "/")),
        shell_mode: launch_config.shell_mode,
        interactive: launch_config.interactive,
        target_ownership: None,
        persistent: launch_config.persistent,
        timeout_ms: launch_config.timeout_ms,
        after_cursor: None,
        since_last_read: false,
        max_bytes: None,
        tail_lines: None,
        stream: None,
        filter: None,
        input: None,
        wait_pattern: None,
        wait_port: None,
        wait_url: None,
        signal: None,
    }
}

#[derive(Debug, Clone)]
struct SystemProcessInfo {
    pid: u32,
    parent_pid: Option<u32>,
    name: String,
    executable_path: Option<String>,
    cwd: Option<String>,
    argv: Vec<String>,
}

#[derive(Debug, Clone)]
struct SystemPortInfo {
    protocol: String,
    local_addr: String,
    local_port: u16,
    state: String,
    pid: Option<u32>,
    process_name: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct ExternalSignal {
    label: &'static str,
    number: i32,
}

fn list_system_processes() -> CommandResult<Vec<SystemProcessInfo>> {
    #[cfg(target_os = "linux")]
    {
        linux_system_processes()
    }

    #[cfg(target_os = "macos")]
    {
        macos_system_processes()
    }

    #[cfg(windows)]
    {
        windows_system_processes()
    }

    #[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
    {
        ps_system_processes()
    }

    #[cfg(not(any(unix, windows)))]
    {
        Err(CommandError::user_fixable(
            "autonomous_tool_process_manager_system_process_unsupported",
            "Xero system process inspection is not supported on this platform yet.",
        ))
    }
}

#[cfg(target_os = "linux")]
fn linux_system_processes() -> CommandResult<Vec<SystemProcessInfo>> {
    let entries = fs::read_dir("/proc").map_err(|error| {
        CommandError::system_fault(
            "autonomous_tool_process_manager_system_process_failed",
            format!("Xero could not read /proc for process inspection: {error}"),
        )
    })?;
    let mut processes = Vec::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(pid) = file_name
            .to_str()
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let root = entry.path();
        let name = fs::read_to_string(root.join("comm"))
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| pid.to_string());
        let parent_pid = linux_parent_pid(&root);
        let executable_path = fs::read_link(root.join("exe"))
            .ok()
            .map(|path| path.display().to_string());
        let cwd = fs::read_link(root.join("cwd"))
            .ok()
            .map(|path| path.display().to_string());
        let argv = fs::read(root.join("cmdline"))
            .ok()
            .map(split_nul_args)
            .filter(|args| !args.is_empty())
            .unwrap_or_else(|| vec![name.clone()]);
        processes.push(SystemProcessInfo {
            pid,
            parent_pid,
            name,
            executable_path,
            cwd,
            argv,
        });
    }
    Ok(processes)
}

#[cfg(target_os = "linux")]
fn linux_parent_pid(root: &std::path::Path) -> Option<u32> {
    let status = fs::read_to_string(root.join("status")).ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("PPid:")
            .and_then(|value| value.trim().parse::<u32>().ok())
    })
}

#[cfg(target_os = "linux")]
fn split_nul_args(bytes: Vec<u8>) -> Vec<String> {
    bytes
        .split(|byte| *byte == 0)
        .filter_map(|part| {
            if part.is_empty() {
                None
            } else {
                Some(String::from_utf8_lossy(part).into_owned())
            }
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn macos_system_processes() -> CommandResult<Vec<SystemProcessInfo>> {
    ps_system_processes()
}

#[cfg(unix)]
fn ps_system_processes() -> CommandResult<Vec<SystemProcessInfo>> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,comm="])
        .output()
        .map_err(|error| {
            CommandError::system_fault(
                "autonomous_tool_process_manager_system_process_failed",
                format!("Xero could not execute ps for process inspection: {error}"),
            )
        })?;
    if !output.status.success() {
        return Err(CommandError::system_fault(
            "autonomous_tool_process_manager_system_process_failed",
            format!("ps exited with status {}.", output.status),
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut processes = Vec::new();
    for line in stdout.lines() {
        let mut parts = line.split_whitespace();
        let Some(pid) = parts.next().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        let parent_pid = parts.next().and_then(|value| value.parse::<u32>().ok());
        let command = parts.collect::<Vec<_>>().join(" ");
        let executable_path = process_executable_path(pid).or_else(|| {
            if command.starts_with('/') {
                Some(command.clone())
            } else {
                None
            }
        });
        let name = executable_path
            .as_deref()
            .or_else(|| (!command.is_empty()).then_some(command.as_str()))
            .and_then(|value| {
                std::path::Path::new(value)
                    .file_name()
                    .and_then(|name| name.to_str())
            })
            .unwrap_or(if command.is_empty() {
                "unknown"
            } else {
                &command
            })
            .to_owned();
        let argv = if command.is_empty() {
            vec![name.clone()]
        } else {
            vec![command.clone()]
        };
        processes.push(SystemProcessInfo {
            pid,
            parent_pid,
            name,
            executable_path,
            cwd: None,
            argv,
        });
    }
    Ok(processes)
}

#[cfg(target_os = "macos")]
fn process_executable_path(pid: u32) -> Option<String> {
    const PROC_PIDPATHINFO_MAXSIZE: usize = 4096;
    extern "C" {
        fn proc_pidpath(
            pid: libc::c_int,
            buffer: *mut libc::c_void,
            buffersize: u32,
        ) -> libc::c_int;
    }

    let mut buffer = vec![0_u8; PROC_PIDPATHINFO_MAXSIZE];
    let result = unsafe {
        proc_pidpath(
            pid as libc::c_int,
            buffer.as_mut_ptr().cast::<libc::c_void>(),
            buffer.len() as u32,
        )
    };
    if result <= 0 {
        return None;
    }
    let len = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(result as usize);
    Some(String::from_utf8_lossy(&buffer[..len]).into_owned())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn process_executable_path(_pid: u32) -> Option<String> {
    None
}

#[cfg(any(windows, test))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WindowsProcessJson {
    process_id: Option<u32>,
    parent_process_id: Option<u32>,
    name: Option<String>,
    executable_path: Option<String>,
    command_line: Option<String>,
}

#[cfg(windows)]
fn windows_system_processes() -> CommandResult<Vec<SystemProcessInfo>> {
    const SCRIPT: &str = "Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name,ExecutablePath,CommandLine | ConvertTo-Json -Depth 2 -Compress";
    match windows_powershell_output(SCRIPT).and_then(|stdout| parse_windows_process_json(&stdout)) {
        Ok(processes) if !processes.is_empty() => return Ok(processes),
        Ok(_) => {}
        Err(_) => {}
    }

    let output = Command::new("tasklist")
        .args(["/fo", "csv", "/v"])
        .output()
        .map_err(|error| {
            CommandError::system_fault(
                "autonomous_tool_process_manager_system_process_failed",
                format!("Xero could not execute tasklist for process inspection: {error}"),
            )
        })?;
    if !output.status.success() {
        return Err(CommandError::system_fault(
            "autonomous_tool_process_manager_system_process_failed",
            format!("tasklist exited with status {}.", output.status),
        ));
    }
    parse_windows_tasklist_csv(&String::from_utf8_lossy(&output.stdout)).map_err(|error| {
        CommandError::system_fault(
            "autonomous_tool_process_manager_system_process_failed",
            format!("Xero could not parse tasklist process output: {error}"),
        )
    })
}

#[cfg(any(windows, test))]
fn parse_windows_process_json(text: &str) -> Result<Vec<SystemProcessInfo>, String> {
    let rows = parse_powershell_json_rows::<WindowsProcessJson>(text)?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let pid = row.process_id?;
            if pid == 0 {
                return None;
            }
            let name = row
                .name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("unknown")
                .to_owned();
            let command_line = row
                .command_line
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let argv = command_line
                .map(|value| vec![value.to_owned()])
                .unwrap_or_else(|| vec![name.clone()]);
            Some(SystemProcessInfo {
                pid,
                parent_pid: row.parent_process_id.filter(|parent| *parent != 0),
                name,
                executable_path: row
                    .executable_path
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty()),
                cwd: None,
                argv,
            })
        })
        .collect())
}

#[cfg(any(windows, test))]
fn parse_windows_tasklist_csv(text: &str) -> Result<Vec<SystemProcessInfo>, String> {
    let mut lines = text.lines();
    let Some(header) = lines.next() else {
        return Ok(Vec::new());
    };
    let headers = parse_windows_csv_record(header);
    let image_index = headers
        .iter()
        .position(|value| value.eq_ignore_ascii_case("Image Name"))
        .unwrap_or(0);
    let pid_index = headers
        .iter()
        .position(|value| value.eq_ignore_ascii_case("PID"))
        .unwrap_or(1);

    let mut processes = Vec::new();
    for line in lines {
        let columns = parse_windows_csv_record(line);
        let Some(pid) = columns
            .get(pid_index)
            .and_then(|value| value.trim().parse::<u32>().ok())
        else {
            continue;
        };
        let name = columns
            .get(image_index)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or("unknown")
            .to_owned();
        processes.push(SystemProcessInfo {
            pid,
            parent_pid: None,
            name: name.clone(),
            executable_path: None,
            cwd: None,
            argv: vec![name],
        });
    }
    Ok(processes)
}

#[cfg(any(windows, test))]
fn parse_windows_csv_record(line: &str) -> Vec<String> {
    let mut columns = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut quoted = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                current.push('"');
                let _ = chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                columns.push(current.clone());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    columns.push(current);
    columns
}

fn filter_system_processes(
    processes: &mut Vec<SystemProcessInfo>,
    request: &AutonomousProcessManagerRequest,
    port_pids: Option<&BTreeSet<u32>>,
) -> CommandResult<()> {
    if let Some(pid) = request
        .pid
        .or_else(|| parse_process_id_pid(request.process_id.as_deref()))
    {
        processes.retain(|process| process.pid == pid);
    }
    if let Some(parent_pid) = request.parent_pid {
        processes.retain(|process| process.parent_pid == Some(parent_pid));
    }
    if let Some(port_pids) = port_pids {
        processes.retain(|process| port_pids.contains(&process.pid));
    }
    if let Some(filter) = request.filter.as_deref() {
        let regex = Regex::new(filter).map_err(|error| {
            CommandError::user_fixable(
                "autonomous_tool_process_manager_filter_invalid",
                format!("Xero could not compile system process filter regex: {error}"),
            )
        })?;
        processes.retain(|process| {
            regex.is_match(&process.name)
                || process
                    .executable_path
                    .as_deref()
                    .is_some_and(|value| regex.is_match(value))
                || process
                    .cwd
                    .as_deref()
                    .is_some_and(|value| regex.is_match(value))
                || process.argv.iter().any(|value| regex.is_match(value))
        });
    }
    Ok(())
}

fn system_process_by_pid(
    processes: &[SystemProcessInfo],
    pid: u32,
) -> CommandResult<SystemProcessInfo> {
    processes
        .iter()
        .find(|process| process.pid == pid)
        .cloned()
        .ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_tool_process_manager_system_process_not_found",
                format!("Xero could not find external/system process PID {pid}."),
            )
        })
}

fn system_process_tree(
    processes: &[SystemProcessInfo],
    target_pid: u32,
) -> CommandResult<Vec<SystemProcessInfo>> {
    let target = system_process_by_pid(processes, target_pid)?;
    let process_by_pid = processes
        .iter()
        .map(|process| (process.pid, process.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut selected = BTreeMap::new();
    selected.insert(target.pid, target.clone());

    let mut parent = target.parent_pid;
    while let Some(parent_pid) = parent {
        let Some(process) = process_by_pid.get(&parent_pid) else {
            break;
        };
        if selected.len() >= MAX_SYSTEM_TREE_PROCESSES || selected.contains_key(&process.pid) {
            break;
        }
        selected.insert(process.pid, process.clone());
        parent = process.parent_pid;
    }

    let mut frontier = vec![target_pid];
    while let Some(parent_pid) = frontier.pop() {
        if selected.len() >= MAX_SYSTEM_TREE_PROCESSES {
            break;
        }
        for child in processes
            .iter()
            .filter(|process| process.parent_pid == Some(parent_pid))
        {
            if selected.insert(child.pid, child.clone()).is_none() {
                frontier.push(child.pid);
            }
        }
    }

    let mut out = selected.into_values().collect::<Vec<_>>();
    out.sort_by_key(|process| {
        if process.pid == target_pid {
            (0_u8, process.pid)
        } else if is_descendant(processes, target_pid, process.pid) {
            (1_u8, process.pid)
        } else {
            (2_u8, process.pid)
        }
    });
    Ok(out)
}

fn is_descendant(processes: &[SystemProcessInfo], ancestor_pid: u32, pid: u32) -> bool {
    let parent_by_pid = processes
        .iter()
        .filter_map(|process| process.parent_pid.map(|parent| (process.pid, parent)))
        .collect::<BTreeMap<_, _>>();
    let mut current = Some(pid);
    let mut seen = BTreeSet::new();
    while let Some(candidate) = current {
        if !seen.insert(candidate) {
            return false;
        }
        if candidate == ancestor_pid {
            return true;
        }
        current = parent_by_pid.get(&candidate).copied();
    }
    false
}

fn system_process_metadata(
    process: &SystemProcessInfo,
    owned_pids: &BTreeSet<u32>,
    ports_by_pid: &BTreeMap<u32, Vec<u16>>,
) -> AutonomousProcessMetadata {
    let scope = if owned_pids.contains(&process.pid) {
        AutonomousProcessOwnershipScope::XeroOwned
    } else {
        AutonomousProcessOwnershipScope::External
    };
    let detected_ports = ports_by_pid.get(&process.pid).cloned().unwrap_or_default();
    AutonomousProcessMetadata {
        process_id: format!("system-pid-{}", process.pid),
        pid: Some(process.pid),
        parent_pid: process.parent_pid,
        process_group_id: None,
        process_name: Some(process.name.clone()),
        executable_path: process.executable_path.clone(),
        label: Some(process.name.clone()),
        process_type: Some("system_process".into()),
        group: None,
        owner: AutonomousProcessOwner {
            thread_id: None,
            session_id: None,
            repo_id: None,
            user_id: None,
            scope,
        },
        command: AutonomousProcessCommandMetadata {
            argv: redact_command_argv_for_persistence(&process.argv),
            shell_mode: false,
            cwd: process.cwd.clone().unwrap_or_else(|| "unknown".into()),
            sanitized_env: Vec::new(),
        },
        stdin_state: AutonomousProcessStdinState::Unavailable,
        status: if process_exists(process.pid) {
            AutonomousProcessStatus::Running
        } else {
            AutonomousProcessStatus::Unknown
        },
        started_at: None,
        exited_at: None,
        exit_code: None,
        output_cursor: None,
        detected_urls: Vec::new(),
        detected_ports,
        recent_errors: Vec::new(),
        recent_warnings: Vec::new(),
        recent_stack_traces: Vec::new(),
        status_changes: Vec::new(),
        readiness: AutonomousProcessReadinessState {
            ready: false,
            detector: None,
            matched: None,
        },
        restart_count: 0,
        last_restart_reason: None,
        async_job: false,
        timeout_ms: None,
        output_artifact: None,
    }
}

fn normalized_system_pid(request: &AutonomousProcessManagerRequest) -> CommandResult<u32> {
    let pid = request
        .pid
        .or_else(|| parse_process_id_pid(request.process_id.as_deref()))
        .ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_tool_process_manager_pid_required",
                "Xero requires system process actions to include pid or a numeric processId.",
            )
        })?;
    if pid == 0 {
        return Err(CommandError::user_fixable(
            "autonomous_tool_process_manager_pid_invalid",
            "Xero requires system process pid to be greater than zero.",
        ));
    }
    Ok(pid)
}

fn parse_process_id_pid(process_id: Option<&str>) -> Option<u32> {
    let value = process_id?.trim();
    value
        .strip_prefix("system-pid-")
        .unwrap_or(value)
        .parse::<u32>()
        .ok()
}

fn system_ports_by_pid(ports: Vec<SystemPortInfo>) -> BTreeMap<u32, Vec<u16>> {
    let mut by_pid: BTreeMap<u32, BTreeSet<u16>> = BTreeMap::new();
    for port in ports {
        if let Some(pid) = port.pid {
            by_pid.entry(pid).or_default().insert(port.local_port);
        }
    }
    by_pid
        .into_iter()
        .map(|(pid, ports)| (pid, ports.into_iter().collect()))
        .collect()
}

fn list_system_ports() -> CommandResult<Vec<SystemPortInfo>> {
    #[cfg(target_os = "linux")]
    {
        linux_system_ports()
    }

    #[cfg(target_os = "macos")]
    {
        lsof_system_ports()
    }

    #[cfg(windows)]
    {
        windows_system_ports()
    }

    #[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
    {
        lsof_system_ports()
    }

    #[cfg(not(any(unix, windows)))]
    {
        Err(CommandError::user_fixable(
            "autonomous_tool_process_manager_system_ports_unsupported",
            "Xero system port inspection is not supported on this platform yet.",
        ))
    }
}

#[cfg(target_os = "linux")]
fn linux_system_ports() -> CommandResult<Vec<SystemPortInfo>> {
    let inode_pids = linux_socket_inode_pids();
    let mut ports = Vec::new();
    ports.extend(linux_tcp_ports("/proc/net/tcp", false, &inode_pids)?);
    ports.extend(linux_tcp_ports("/proc/net/tcp6", true, &inode_pids)?);
    Ok(ports)
}

#[cfg(target_os = "linux")]
fn linux_socket_inode_pids() -> BTreeMap<String, u32> {
    let mut out = BTreeMap::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return out;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(pid) = file_name
            .to_str()
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let fd_dir = entry.path().join("fd");
        let Ok(fds) = fs::read_dir(fd_dir) else {
            continue;
        };
        for fd in fds.flatten() {
            let Ok(target) = fs::read_link(fd.path()) else {
                continue;
            };
            let target = target.to_string_lossy();
            if let Some(inode) = target
                .strip_prefix("socket:[")
                .and_then(|value| value.strip_suffix(']'))
            {
                out.entry(inode.to_owned()).or_insert(pid);
            }
        }
    }
    out
}

#[cfg(target_os = "linux")]
fn linux_tcp_ports(
    path: &str,
    ipv6: bool,
    inode_pids: &BTreeMap<String, u32>,
) -> CommandResult<Vec<SystemPortInfo>> {
    let content = fs::read_to_string(path).map_err(|error| {
        CommandError::system_fault(
            "autonomous_tool_process_manager_system_ports_failed",
            format!("Xero could not read {path} for listening ports: {error}"),
        )
    })?;
    let mut ports = Vec::new();
    for line in content.lines().skip(1) {
        let columns = line.split_whitespace().collect::<Vec<_>>();
        if columns.len() < 10 || columns[3] != "0A" {
            continue;
        }
        let Some((addr_hex, port_hex)) = columns[1].split_once(':') else {
            continue;
        };
        let Ok(local_port) = u16::from_str_radix(port_hex, 16) else {
            continue;
        };
        let local_addr = if ipv6 {
            linux_ipv6_addr(addr_hex)
        } else {
            linux_ipv4_addr(addr_hex)
        };
        let inode = columns[9].to_owned();
        let pid = inode_pids.get(&inode).copied();
        ports.push(SystemPortInfo {
            protocol: if ipv6 { "tcp6" } else { "tcp" }.into(),
            local_addr,
            local_port,
            state: "listen".into(),
            pid,
            process_name: None,
        });
    }
    Ok(ports)
}

#[cfg(target_os = "linux")]
fn linux_ipv4_addr(value: &str) -> String {
    let Ok(raw) = u32::from_str_radix(value, 16) else {
        return value.into();
    };
    let bytes = raw.to_le_bytes();
    format!("{}.{}.{}.{}", bytes[0], bytes[1], bytes[2], bytes[3])
}

#[cfg(target_os = "linux")]
fn linux_ipv6_addr(value: &str) -> String {
    if value.len() != 32 {
        return value.into();
    }
    let mut segments = Vec::new();
    for chunk in value.as_bytes().chunks(8) {
        let chunk = String::from_utf8_lossy(chunk);
        let Ok(raw) = u32::from_str_radix(&chunk, 16) else {
            return value.into();
        };
        for segment in raw.to_le_bytes().chunks(2) {
            segments.push(u16::from_be_bytes([segment[0], segment[1]]));
        }
    }
    segments
        .chunks(1)
        .map(|chunk| format!("{:x}", chunk[0]))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(unix)]
fn lsof_system_ports() -> CommandResult<Vec<SystemPortInfo>> {
    let output = Command::new("lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-F", "pcPn"])
        .output()
        .map_err(|error| {
            CommandError::system_fault(
                "autonomous_tool_process_manager_system_ports_failed",
                format!("Xero could not execute lsof for listening ports: {error}"),
            )
        })?;
    if !output.status.success() && output.stdout.is_empty() {
        return Err(CommandError::system_fault(
            "autonomous_tool_process_manager_system_ports_failed",
            format!("lsof exited with status {}.", output.status),
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut ports = Vec::new();
    let mut pid = None;
    let mut process_name = None;
    let mut protocol = "tcp".to_owned();
    for line in stdout.lines() {
        if line.is_empty() {
            continue;
        }
        let (tag, value) = line.split_at(1);
        match tag {
            "p" => {
                pid = value.parse::<u32>().ok();
                process_name = None;
                protocol = "tcp".into();
            }
            "c" => process_name = Some(value.to_owned()),
            "P" => protocol = value.to_ascii_lowercase(),
            "n" => {
                if let Some((addr, port)) = parse_lsof_address(value) {
                    ports.push(SystemPortInfo {
                        protocol: protocol.clone(),
                        local_addr: addr,
                        local_port: port,
                        state: "listen".into(),
                        pid,
                        process_name: process_name.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    Ok(ports)
}

fn parse_lsof_address(value: &str) -> Option<(String, u16)> {
    let without_state = value.split(" (").next().unwrap_or(value);
    let (addr, port) = if let Some(end) = without_state.rfind("]:") {
        let addr = without_state[..=end]
            .trim_start_matches('[')
            .trim_end_matches(']');
        (addr.to_owned(), &without_state[end + 2..])
    } else {
        let (addr, port) = without_state.rsplit_once(':')?;
        (addr.to_owned(), port)
    };
    let port = port.parse::<u16>().ok()?;
    Some((addr, port))
}

#[cfg(any(windows, test))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WindowsNetTcpConnectionJson {
    local_address: Option<String>,
    local_port: Option<serde_json::Value>,
    state: Option<serde_json::Value>,
    owning_process: Option<serde_json::Value>,
}

#[cfg(windows)]
fn windows_system_ports() -> CommandResult<Vec<SystemPortInfo>> {
    const SCRIPT: &str = "Get-NetTCPConnection -State Listen | Select-Object LocalAddress,LocalPort,State,OwningProcess | ConvertTo-Json -Depth 2 -Compress";
    let mut ports = match windows_powershell_output(SCRIPT)
        .and_then(|stdout| parse_windows_net_tcp_json(&stdout))
    {
        Ok(ports) => ports,
        Err(_) => {
            let output = Command::new("netstat")
                .args(["-ano", "-p", "tcp"])
                .output()
                .map_err(|error| {
                    CommandError::system_fault(
                        "autonomous_tool_process_manager_system_ports_failed",
                        format!("Xero could not execute netstat for listening ports: {error}"),
                    )
                })?;
            if !output.status.success() {
                return Err(CommandError::system_fault(
                    "autonomous_tool_process_manager_system_ports_failed",
                    format!("netstat exited with status {}.", output.status),
                ));
            }
            parse_windows_netstat(&String::from_utf8_lossy(&output.stdout)).map_err(|error| {
                CommandError::system_fault(
                    "autonomous_tool_process_manager_system_ports_failed",
                    format!("Xero could not parse netstat output: {error}"),
                )
            })?
        }
    };

    let names = windows_system_processes()
        .unwrap_or_default()
        .into_iter()
        .map(|process| (process.pid, process.name))
        .collect::<BTreeMap<_, _>>();
    for port in &mut ports {
        if port.process_name.is_none() {
            port.process_name = port.pid.and_then(|pid| names.get(&pid).cloned());
        }
    }
    Ok(ports)
}

#[cfg(any(windows, test))]
fn parse_windows_net_tcp_json(text: &str) -> Result<Vec<SystemPortInfo>, String> {
    let rows = parse_powershell_json_rows::<WindowsNetTcpConnectionJson>(text)?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let local_port = row.local_port.as_ref().and_then(json_value_u16)?;
            let pid = row.owning_process.as_ref().and_then(json_value_u32);
            let state = row
                .state
                .as_ref()
                .and_then(json_value_string)
                .unwrap_or_else(|| "Listen".into());
            let local_addr = row
                .local_address
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "0.0.0.0".into());
            let protocol = if local_addr.contains(':') {
                "tcp6"
            } else {
                "tcp"
            };
            Some(SystemPortInfo {
                protocol: protocol.into(),
                local_addr,
                local_port,
                state: state.to_ascii_lowercase(),
                pid,
                process_name: None,
            })
        })
        .collect())
}

#[cfg(any(windows, test))]
fn parse_windows_netstat(text: &str) -> Result<Vec<SystemPortInfo>, String> {
    let mut ports = Vec::new();
    for line in text.lines() {
        let columns = line.split_whitespace().collect::<Vec<_>>();
        if columns.len() < 5 || !columns[0].eq_ignore_ascii_case("TCP") {
            continue;
        }
        let state = columns[3];
        if !state.eq_ignore_ascii_case("LISTENING") {
            continue;
        }
        let Some((local_addr, local_port)) = parse_windows_addr_port(columns[1]) else {
            continue;
        };
        let pid = columns[4].parse::<u32>().ok();
        let protocol = if local_addr.contains(':') {
            "tcp6"
        } else {
            "tcp"
        };
        ports.push(SystemPortInfo {
            protocol: protocol.into(),
            local_addr,
            local_port,
            state: "listen".into(),
            pid,
            process_name: None,
        });
    }
    Ok(ports)
}

#[cfg(any(windows, test))]
fn parse_windows_addr_port(value: &str) -> Option<(String, u16)> {
    if let Some(end) = value.rfind("]:") {
        let addr = value[..=end]
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_owned();
        let port = value[end + 2..].parse::<u16>().ok()?;
        return Some((addr, port));
    }
    let (addr, port) = value.rsplit_once(':')?;
    Some((addr.to_owned(), port.parse::<u16>().ok()?))
}

#[cfg(any(windows, test))]
fn parse_powershell_json_rows<T>(text: &str) -> Result<Vec<T>, String>
where
    T: serde::de::DeserializeOwned,
{
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let value = serde_json::from_str::<serde_json::Value>(trimmed)
        .map_err(|error| format!("invalid PowerShell JSON: {error}"))?;
    if value.is_null() {
        return Ok(Vec::new());
    }
    if value.is_array() {
        serde_json::from_value(value).map_err(|error| format!("invalid row array: {error}"))
    } else {
        serde_json::from_value(value)
            .map(|row| vec![row])
            .map_err(|error| format!("invalid row object: {error}"))
    }
}

#[cfg(any(windows, test))]
fn json_value_u16(value: &serde_json::Value) -> Option<u16> {
    value
        .as_u64()
        .and_then(|number| u16::try_from(number).ok())
        .or_else(|| value.as_str()?.parse::<u16>().ok())
}

#[cfg(any(windows, test))]
fn json_value_u32(value: &serde_json::Value) -> Option<u32> {
    value
        .as_u64()
        .and_then(|number| u32::try_from(number).ok())
        .or_else(|| value.as_str()?.parse::<u32>().ok())
}

#[cfg(any(windows, test))]
fn json_value_string(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| value.as_u64().map(|number| number.to_string()))
}

#[cfg(windows)]
fn windows_powershell_output(script: &str) -> Result<String, String> {
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|error| format!("failed to spawn powershell.exe: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "powershell.exe exited with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn filter_system_ports(
    ports: &mut Vec<SystemPortInfo>,
    request: &AutonomousProcessManagerRequest,
) -> CommandResult<()> {
    if let Some(port) = request.port.or(request.wait_port) {
        ports.retain(|entry| entry.local_port == port);
    }
    if let Some(pid) = request
        .pid
        .or_else(|| parse_process_id_pid(request.process_id.as_deref()))
    {
        ports.retain(|entry| entry.pid == Some(pid));
    }
    if let Some(filter) = request.filter.as_deref() {
        let regex = Regex::new(filter).map_err(|error| {
            CommandError::user_fixable(
                "autonomous_tool_process_manager_filter_invalid",
                format!("Xero could not compile system port filter regex: {error}"),
            )
        })?;
        ports.retain(|entry| {
            regex.is_match(&entry.protocol)
                || regex.is_match(&entry.local_addr)
                || entry
                    .process_name
                    .as_deref()
                    .is_some_and(|value| regex.is_match(value))
        });
    }
    Ok(())
}

fn system_port_output(port: &SystemPortInfo) -> AutonomousSystemPort {
    AutonomousSystemPort {
        protocol: port.protocol.clone(),
        local_addr: port.local_addr.clone(),
        local_port: port.local_port,
        state: port.state.clone(),
        pid: port.pid,
        process_name: port.process_name.clone(),
    }
}

fn normalized_external_signal(signal: Option<&str>) -> CommandResult<ExternalSignal> {
    let normalized = signal
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("TERM")
        .trim_start_matches("SIG")
        .to_ascii_uppercase();
    #[cfg(unix)]
    {
        let signal = match normalized.as_str() {
            "TERM" => ExternalSignal {
                label: "TERM",
                number: libc::SIGTERM,
            },
            "KILL" => ExternalSignal {
                label: "KILL",
                number: libc::SIGKILL,
            },
            "INT" => ExternalSignal {
                label: "INT",
                number: libc::SIGINT,
            },
            "HUP" => ExternalSignal {
                label: "HUP",
                number: libc::SIGHUP,
            },
            "QUIT" => ExternalSignal {
                label: "QUIT",
                number: libc::SIGQUIT,
            },
            "USR1" => ExternalSignal {
                label: "USR1",
                number: libc::SIGUSR1,
            },
            "USR2" => ExternalSignal {
                label: "USR2",
                number: libc::SIGUSR2,
            },
            "STOP" => ExternalSignal {
                label: "STOP",
                number: libc::SIGSTOP,
            },
            "CONT" => ExternalSignal {
                label: "CONT",
                number: libc::SIGCONT,
            },
            _ => {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_process_manager_signal_invalid",
                    "Xero supports external signals TERM, KILL, INT, HUP, QUIT, USR1, USR2, STOP, and CONT.",
                ));
            }
        };
        Ok(signal)
    }
    #[cfg(windows)]
    {
        let signal = match normalized.as_str() {
            "TERM" | "TERMINATE" | "INT" => ExternalSignal {
                label: "TERM",
                number: 15,
            },
            "KILL" | "FORCE" => ExternalSignal {
                label: "KILL",
                number: 9,
            },
            _ => {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_process_manager_signal_invalid",
                    "Xero supports external Windows signals TERM and KILL.",
                ));
            }
        };
        Ok(signal)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = normalized;
        Err(CommandError::user_fixable(
            "autonomous_tool_process_manager_signal_unsupported",
            "Xero external process signals are not supported on this platform yet.",
        ))
    }
}

fn ensure_external_signal_target(
    target_pid: u32,
    processes: &[SystemProcessInfo],
    owned_pids: &BTreeSet<u32>,
    include_descendants: bool,
) -> CommandResult<()> {
    if owned_pids.contains(&target_pid) {
        return Err(CommandError::user_fixable(
            "autonomous_tool_process_manager_owned_target_refused",
            "Xero refused an external signal request for a Xero-owned process. Use the owned kill/group actions instead.",
        ));
    }
    let current_pid = std::process::id();
    if target_pid == current_pid {
        return Err(CommandError::policy_denied(
            "Xero refused to signal its own desktop process.",
        ));
    }
    if current_process_ancestors(processes).contains(&target_pid) {
        return Err(CommandError::policy_denied(
            "Xero refused to signal an ancestor of its own desktop process.",
        ));
    }
    if include_descendants {
        if is_descendant(processes, target_pid, current_pid) {
            return Err(CommandError::policy_denied(
                "Xero refused to kill a process tree that contains its own desktop process.",
            ));
        }
        if owned_pids
            .iter()
            .any(|owned_pid| is_descendant(processes, target_pid, *owned_pid))
        {
            return Err(CommandError::user_fixable(
                "autonomous_tool_process_manager_owned_target_refused",
                "Xero refused an external tree kill that includes Xero-owned processes. Use owned process-manager actions for those targets.",
            ));
        }
    }
    Ok(())
}

fn current_process_ancestors(processes: &[SystemProcessInfo]) -> BTreeSet<u32> {
    let parent_by_pid = processes
        .iter()
        .filter_map(|process| process.parent_pid.map(|parent| (process.pid, parent)))
        .collect::<BTreeMap<_, _>>();
    let mut ancestors = BTreeSet::new();
    let mut current = parent_by_pid.get(&std::process::id()).copied();
    while let Some(pid) = current {
        if !ancestors.insert(pid) {
            break;
        }
        current = parent_by_pid.get(&pid).copied();
    }
    ancestors
}

fn signal_external_pid(pid: u32, signal: i32) -> CommandResult<()> {
    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(pid as libc::pid_t, signal) };
        if result == 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        Err(CommandError::retryable(
            "autonomous_tool_process_manager_system_signal_failed",
            format!("Xero could not signal external PID {pid}: {error}"),
        ))
    }
    #[cfg(windows)]
    {
        windows_taskkill(pid, signal == 9)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (pid, signal);
        Err(CommandError::user_fixable(
            "autonomous_tool_process_manager_signal_unsupported",
            "Xero external process signals are not supported on this platform yet.",
        ))
    }
}

fn kill_external_process_tree(
    tree: &[SystemProcessInfo],
    root_pid: u32,
) -> CommandResult<BTreeSet<u32>> {
    let mut targets = tree
        .iter()
        .filter(|process| is_descendant(tree, root_pid, process.pid) || process.pid == root_pid)
        .map(|process| process.pid)
        .collect::<Vec<_>>();
    targets.sort_by(
        |left, right| match (*left == root_pid, *right == root_pid) {
            (true, true) => left.cmp(right),
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            (false, false) => right.cmp(left),
        },
    );
    targets.dedup();
    let killed = targets.iter().copied().collect::<BTreeSet<_>>();

    for pid in &targets {
        let _ = signal_external_pid(*pid, normalized_external_signal(Some("TERM"))?.number);
    }
    thread::sleep(Duration::from_millis(300));
    for pid in &targets {
        if process_exists(*pid) {
            let _ = signal_external_pid(*pid, normalized_external_signal(Some("KILL"))?.number);
        }
    }
    Ok(killed)
}

fn process_exists(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
        if result == 0 {
            return true;
        }
        std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }
    #[cfg(windows)]
    {
        windows_tasklist_has_pid(pid)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

#[cfg(windows)]
fn windows_taskkill(pid: u32, force: bool) -> CommandResult<()> {
    let mut command = Command::new("taskkill");
    command.arg("/PID").arg(pid.to_string()).arg("/T");
    if force {
        command.arg("/F");
    }
    let output = command.output().map_err(|error| {
        CommandError::retryable(
            "autonomous_tool_process_manager_system_signal_failed",
            format!("Xero could not spawn taskkill for external PID {pid}: {error}"),
        )
    })?;
    if output.status.success() {
        return Ok(());
    }
    Err(CommandError::retryable(
        "autonomous_tool_process_manager_system_signal_failed",
        format!(
            "taskkill failed for external PID {pid} with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ),
    ))
}

#[cfg(windows)]
fn windows_tasklist_has_pid(pid: u32) -> bool {
    let Ok(output) = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    !stdout.contains("INFO:")
        && parse_windows_tasklist_csv(&format!("\"Image Name\",\"PID\"\n{stdout}"))
            .is_ok_and(|processes| processes.iter().any(|process| process.pid == pid))
}

struct ProcessOutputRead {
    chunks: Vec<AutonomousProcessOutputChunk>,
    advance_cursor: Option<u64>,
}

fn read_process_output_for_request(
    process: &OwnedProcess,
    request: &AutonomousProcessManagerRequest,
) -> CommandResult<ProcessOutputRead> {
    let after_cursor = request.after_cursor.unwrap_or_else(|| {
        if request.since_last_read {
            process.last_read_cursor_value()
        } else {
            0
        }
    });
    let max_bytes = request
        .max_bytes
        .unwrap_or_else(default_process_output_read_bytes)
        .clamp(1, MAX_PROCESS_OUTPUT_READ_BYTES);
    let selected_chunks = process.read_chunks_after(after_cursor, max_bytes)?;
    let mut chunks = selected_chunks.clone();

    if let Some(stream) = request.stream {
        if stream != AutonomousProcessOutputStream::Combined {
            chunks.retain(|chunk| chunk.stream == stream);
        }
    }

    if let Some(filter) = request.filter.as_deref() {
        let regex = Regex::new(filter).map_err(|error| {
            CommandError::user_fixable(
                "autonomous_tool_process_manager_filter_invalid",
                format!("Xero could not compile process_manager filter regex: {error}"),
            )
        })?;
        chunks.retain(|chunk| {
            chunk
                .text
                .as_deref()
                .is_some_and(|text| regex.is_match(text))
        });
    }

    if let Some(tail_lines) = request.tail_lines {
        chunks = tail_process_output_chunks(chunks, tail_lines);
    }

    let advance_cursor = returned_output_advance_cursor(after_cursor, &selected_chunks, &chunks);

    Ok(ProcessOutputRead {
        chunks,
        advance_cursor,
    })
}

fn returned_output_advance_cursor(
    after_cursor: u64,
    selected_chunks: &[AutonomousProcessOutputChunk],
    returned_chunks: &[AutonomousProcessOutputChunk],
) -> Option<u64> {
    let mut returned = returned_chunks.iter().peekable();
    let mut advance_cursor = after_cursor;

    for selected in selected_chunks {
        let Some(candidate) = returned.peek() else {
            break;
        };
        if output_chunks_match(selected, candidate) {
            advance_cursor = selected.cursor;
            let _ = returned.next();
        } else {
            break;
        }
    }

    (advance_cursor > after_cursor).then_some(advance_cursor)
}

fn output_chunks_match(
    selected: &AutonomousProcessOutputChunk,
    returned: &AutonomousProcessOutputChunk,
) -> bool {
    selected.cursor == returned.cursor
        && selected.stream == returned.stream
        && selected.text == returned.text
        && selected.truncated == returned.truncated
        && selected.redacted == returned.redacted
}

fn tail_process_output_chunks(
    chunks: Vec<AutonomousProcessOutputChunk>,
    tail_lines: usize,
) -> Vec<AutonomousProcessOutputChunk> {
    let combined = combine_chunk_text(&chunks);
    let lines = combined.lines().collect::<Vec<_>>();
    if lines.len() <= tail_lines {
        return chunks;
    }

    let text = lines[lines.len().saturating_sub(tail_lines)..].join("\n");
    let cursor = chunks.last().map(|chunk| chunk.cursor).unwrap_or_default();
    let captured_at = chunks.last().and_then(|chunk| chunk.captured_at.clone());
    vec![AutonomousProcessOutputChunk {
        cursor,
        stream: AutonomousProcessOutputStream::Combined,
        text: Some(text),
        truncated: true,
        redacted: chunks.iter().any(|chunk| chunk.redacted),
        captured_at,
    }]
}

fn wait_for_output_match(
    process: &OwnedProcess,
    after_cursor: u64,
    wait_pattern: &str,
    timeout: Duration,
) -> CommandResult<(Vec<AutonomousProcessOutputChunk>, Option<String>)> {
    let regex = Regex::new(wait_pattern).map_err(|error| {
        CommandError::user_fixable(
            "autonomous_tool_process_manager_wait_pattern_invalid",
            format!("Xero could not compile process_manager waitPattern regex: {error}"),
        )
    })?;
    let started = Instant::now();

    loop {
        let _ = process.poll_exit()?;
        let raw_chunks =
            process.read_raw_chunks_after(after_cursor, MAX_PROCESS_OUTPUT_READ_BYTES)?;
        let combined = combine_raw_chunk_text(&raw_chunks);
        if let Some(found) = regex.find(&combined) {
            let chunks = process.read_chunks_after(after_cursor, MAX_PROCESS_OUTPUT_READ_BYTES)?;
            return Ok((chunks, Some(found.as_str().to_owned())));
        }

        if started.elapsed() >= timeout {
            let chunks = process.read_chunks_after(after_cursor, MAX_PROCESS_OUTPUT_READ_BYTES)?;
            return Ok((chunks, None));
        }

        thread::sleep(PROCESS_MANAGER_WAIT_POLL);
    }
}

fn combine_chunk_text(chunks: &[AutonomousProcessOutputChunk]) -> String {
    chunks
        .iter()
        .filter_map(|chunk| chunk.text.as_deref())
        .collect::<Vec<_>>()
        .join("\n")
}

fn combine_raw_chunk_text(chunks: &[RawProcessOutputChunk]) -> String {
    chunks
        .iter()
        .map(|chunk| chunk.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn process_digest(processes: &[AutonomousProcessMetadata]) -> String {
    if processes.is_empty() {
        return "No Xero-owned processes are registered.".into();
    }

    processes
        .iter()
        .map(|process| {
            let name = process
                .label
                .as_deref()
                .or(process.process_type.as_deref())
                .unwrap_or("unnamed");
            let readiness = if process.readiness.ready {
                process
                    .readiness
                    .matched
                    .as_deref()
                    .map(|matched| format!("ready:{matched}"))
                    .unwrap_or_else(|| "ready".into())
            } else {
                "not_ready".into()
            };
            let urls = compact_list(&process.detected_urls);
            let ports = process
                .detected_ports
                .iter()
                .map(u16::to_string)
                .collect::<Vec<_>>();
            format!(
                "{} `{}` status={:?} pid={} cursor={} {} urls={} ports={} warnings={} errors={}",
                process.process_id,
                name,
                process.status,
                process
                    .pid
                    .map(|pid| pid.to_string())
                    .unwrap_or_else(|| "unknown".into()),
                process
                    .output_cursor
                    .map(|cursor| cursor.to_string())
                    .unwrap_or_else(|| "0".into()),
                readiness,
                if urls.is_empty() {
                    "none".into()
                } else {
                    urls.join(",")
                },
                if ports.is_empty() {
                    "none".into()
                } else {
                    ports.join(",")
                },
                process.recent_warnings.len(),
                process.recent_errors.len(),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn compact_list(values: &[String]) -> Vec<String> {
    values
        .iter()
        .take(3)
        .map(|value| truncate_chars(value, 96))
        .collect()
}

fn result_highlights(
    action: AutonomousProcessManagerAction,
    processes: &[AutonomousProcessMetadata],
    chunks: &[AutonomousProcessOutputChunk],
) -> Vec<AutonomousProcessHighlight> {
    let mut highlights = Vec::new();
    if !chunks.is_empty() {
        let process_id = processes
            .first()
            .map(|process| process.process_id.as_str())
            .unwrap_or("unknown");
        highlights.extend(extract_process_highlights(process_id, chunks));
    }
    if matches!(
        action,
        AutonomousProcessManagerAction::Digest
            | AutonomousProcessManagerAction::Highlights
            | AutonomousProcessManagerAction::Status
            | AutonomousProcessManagerAction::List
            | AutonomousProcessManagerAction::WaitForReady
            | AutonomousProcessManagerAction::GroupStatus
            | AutonomousProcessManagerAction::AsyncAwait
    ) {
        for process in processes {
            highlights.extend(metadata_highlights(process));
        }
    }
    truncate_highlights(highlights)
}

fn metadata_highlights(process: &AutonomousProcessMetadata) -> Vec<AutonomousProcessHighlight> {
    let mut highlights = Vec::new();
    for url in &process.detected_urls {
        highlights.push(metadata_highlight(
            process,
            AutonomousProcessHighlightKind::Url,
            url.clone(),
        ));
    }
    for port in &process.detected_ports {
        highlights.push(metadata_highlight(
            process,
            AutonomousProcessHighlightKind::Port,
            port.to_string(),
        ));
    }
    for warning in &process.recent_warnings {
        highlights.push(metadata_highlight(
            process,
            AutonomousProcessHighlightKind::Warning,
            warning.clone(),
        ));
    }
    for error in &process.recent_errors {
        highlights.push(metadata_highlight(
            process,
            AutonomousProcessHighlightKind::Error,
            error.clone(),
        ));
    }
    for stack_trace in &process.recent_stack_traces {
        highlights.push(metadata_highlight(
            process,
            AutonomousProcessHighlightKind::StackTrace,
            stack_trace.clone(),
        ));
    }
    for status_change in &process.status_changes {
        highlights.push(metadata_highlight(
            process,
            AutonomousProcessHighlightKind::StatusChange,
            status_change.clone(),
        ));
    }
    if process.readiness.ready {
        highlights.push(metadata_highlight(
            process,
            AutonomousProcessHighlightKind::Readiness,
            process
                .readiness
                .matched
                .clone()
                .unwrap_or_else(|| "ready".into()),
        ));
    }
    highlights
}

fn metadata_highlight(
    process: &AutonomousProcessMetadata,
    kind: AutonomousProcessHighlightKind,
    text: String,
) -> AutonomousProcessHighlight {
    AutonomousProcessHighlight {
        process_id: process.process_id.clone(),
        kind,
        text,
        stream: None,
        cursor: process.output_cursor,
        captured_at: None,
    }
}

fn extract_process_highlights(
    process_id: &str,
    chunks: &[AutonomousProcessOutputChunk],
) -> Vec<AutonomousProcessHighlight> {
    let url_regex = Regex::new(r#"https?://[^\s'"<>)]+"#).expect("valid url regex");
    let port_regex = Regex::new(
        r"(?i)\b(?:localhost|127\.0\.0\.1|0\.0\.0\.0|port|listening|server|ready|started)[^\n]{0,48}\b([1-9][0-9]{1,4})\b",
    )
    .expect("valid port regex");
    let mut seen = BTreeSet::new();
    let mut highlights = Vec::new();

    for chunk in chunks {
        let Some(text) = chunk.text.as_deref() else {
            continue;
        };
        for url_match in url_regex.find_iter(text) {
            let url = trim_url_token(url_match.as_str());
            push_process_highlight(
                &mut highlights,
                &mut seen,
                process_id,
                AutonomousProcessHighlightKind::Url,
                url.clone(),
                chunk,
            );
            if let Some(port) = port_from_url(&url) {
                push_process_highlight(
                    &mut highlights,
                    &mut seen,
                    process_id,
                    AutonomousProcessHighlightKind::Port,
                    port.to_string(),
                    chunk,
                );
            }
        }
        for capture in port_regex.captures_iter(text) {
            if let Some(port) = capture
                .get(1)
                .and_then(|match_| match_.as_str().parse::<u16>().ok())
            {
                push_process_highlight(
                    &mut highlights,
                    &mut seen,
                    process_id,
                    AutonomousProcessHighlightKind::Port,
                    port.to_string(),
                    chunk,
                );
            }
        }
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if is_warning_line(line) {
                push_process_highlight(
                    &mut highlights,
                    &mut seen,
                    process_id,
                    AutonomousProcessHighlightKind::Warning,
                    truncate_chars(line, 240),
                    chunk,
                );
            }
            if is_error_line(line) {
                push_process_highlight(
                    &mut highlights,
                    &mut seen,
                    process_id,
                    AutonomousProcessHighlightKind::Error,
                    truncate_chars(line, 240),
                    chunk,
                );
            }
            if is_stack_trace_line(line) {
                push_process_highlight(
                    &mut highlights,
                    &mut seen,
                    process_id,
                    AutonomousProcessHighlightKind::StackTrace,
                    truncate_chars(line, 240),
                    chunk,
                );
            }
        }
    }

    truncate_highlights(highlights)
}

fn extract_process_network_highlights_from_raw(
    process_id: &str,
    chunks: &[RawProcessOutputChunk],
) -> Vec<AutonomousProcessHighlight> {
    let url_regex = Regex::new(r#"https?://[^\s'"<>)]+"#).expect("valid url regex");
    let port_regex = Regex::new(
        r"(?i)\b(?:localhost|127\.0\.0\.1|0\.0\.0\.0|port|listening|server|ready|started)[^\n]{0,48}\b([1-9][0-9]{1,4})\b",
    )
    .expect("valid port regex");
    let mut seen = BTreeSet::new();
    let mut highlights = Vec::new();

    for chunk in chunks {
        for url_match in url_regex.find_iter(&chunk.text) {
            let url = sanitized_url_highlight(url_match.as_str());
            push_raw_network_highlight(
                &mut highlights,
                &mut seen,
                process_id,
                AutonomousProcessHighlightKind::Url,
                url.clone(),
                chunk,
            );
            if let Some(port) = port_from_url(&url) {
                push_raw_network_highlight(
                    &mut highlights,
                    &mut seen,
                    process_id,
                    AutonomousProcessHighlightKind::Port,
                    port.to_string(),
                    chunk,
                );
            }
        }
        for capture in port_regex.captures_iter(&chunk.text) {
            if let Some(port) = capture
                .get(1)
                .and_then(|match_| match_.as_str().parse::<u16>().ok())
            {
                push_raw_network_highlight(
                    &mut highlights,
                    &mut seen,
                    process_id,
                    AutonomousProcessHighlightKind::Port,
                    port.to_string(),
                    chunk,
                );
            }
        }
    }

    truncate_highlights(highlights)
}

fn push_process_highlight(
    highlights: &mut Vec<AutonomousProcessHighlight>,
    seen: &mut BTreeSet<(AutonomousProcessHighlightKind, String)>,
    process_id: &str,
    kind: AutonomousProcessHighlightKind,
    text: String,
    chunk: &AutonomousProcessOutputChunk,
) {
    if highlights.len() >= MAX_PROCESS_HIGHLIGHTS {
        return;
    }
    let normalized = text.trim().to_owned();
    if normalized.is_empty() || !seen.insert((kind, normalized.clone())) {
        return;
    }
    highlights.push(AutonomousProcessHighlight {
        process_id: process_id.into(),
        kind,
        text: normalized,
        stream: Some(chunk.stream),
        cursor: Some(chunk.cursor),
        captured_at: chunk.captured_at.clone(),
    });
}

fn push_raw_network_highlight(
    highlights: &mut Vec<AutonomousProcessHighlight>,
    seen: &mut BTreeSet<(AutonomousProcessHighlightKind, String)>,
    process_id: &str,
    kind: AutonomousProcessHighlightKind,
    text: String,
    chunk: &RawProcessOutputChunk,
) {
    if highlights.len() >= MAX_PROCESS_HIGHLIGHTS {
        return;
    }
    let normalized = text.trim().to_owned();
    if normalized.is_empty() || !seen.insert((kind, normalized.clone())) {
        return;
    }
    highlights.push(AutonomousProcessHighlight {
        process_id: process_id.into(),
        kind,
        text: normalized,
        stream: Some(chunk.stream),
        cursor: Some(chunk.cursor),
        captured_at: chunk.captured_at.clone(),
    });
}

fn truncate_highlights(
    highlights: Vec<AutonomousProcessHighlight>,
) -> Vec<AutonomousProcessHighlight> {
    highlights
        .into_iter()
        .take(MAX_PROCESS_HIGHLIGHTS)
        .collect()
}

fn unique_highlight_texts(
    highlights: &[AutonomousProcessHighlight],
    kind: AutonomousProcessHighlightKind,
) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut values = Vec::new();
    for highlight in highlights.iter().filter(|highlight| highlight.kind == kind) {
        if seen.insert(highlight.text.clone()) {
            values.push(highlight.text.clone());
        }
        if values.len() >= 8 {
            break;
        }
    }
    values
}

fn unique_highlight_ports(highlights: &[AutonomousProcessHighlight]) -> Vec<u16> {
    let mut ports = BTreeSet::new();
    for highlight in highlights
        .iter()
        .filter(|highlight| highlight.kind == AutonomousProcessHighlightKind::Port)
    {
        if let Ok(port) = highlight.text.parse::<u16>() {
            ports.insert(port);
        }
    }
    ports.into_iter().take(8).collect()
}

fn recent_highlight_texts(
    highlights: &[AutonomousProcessHighlight],
    kind: AutonomousProcessHighlightKind,
) -> Vec<String> {
    highlights
        .iter()
        .rev()
        .filter(|highlight| highlight.kind == kind)
        .take(5)
        .map(|highlight| highlight.text.clone())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn process_status_summaries(
    process_id: &str,
    status: AutonomousProcessStatus,
    exit_code: Option<i32>,
    readiness: AutonomousProcessReadinessState,
) -> Vec<String> {
    let mut summaries = Vec::new();
    if readiness.ready {
        summaries.push(format!(
            "{process_id} ready via {}{}",
            readiness_detector_label(readiness.detector),
            readiness
                .matched
                .as_deref()
                .map(|matched| format!(" ({})", truncate_chars(matched, 120)))
                .unwrap_or_default()
        ));
    }
    if matches!(
        status,
        AutonomousProcessStatus::Exited
            | AutonomousProcessStatus::Failed
            | AutonomousProcessStatus::Killed
    ) {
        summaries.push(format!(
            "{process_id} status={status:?} exit_code={exit_code:?}"
        ));
    }
    summaries
}

fn readiness_detector_label(detector: Option<AutonomousProcessReadinessDetector>) -> &'static str {
    match detector {
        Some(AutonomousProcessReadinessDetector::OutputRegex) => "output_regex",
        Some(AutonomousProcessReadinessDetector::PortOpen) => "port_open",
        Some(AutonomousProcessReadinessDetector::HttpUrl) => "http_url",
        Some(AutonomousProcessReadinessDetector::ProcessExit) => "process_exit",
        None => "unspecified",
    }
}

fn trim_url_token(value: &str) -> String {
    value
        .trim_end_matches(|character: char| {
            matches!(character, '.' | ',' | ';' | ':' | '!' | '?' | ']')
        })
        .to_owned()
}

fn sanitized_url_highlight(value: &str) -> String {
    let trimmed = trim_url_token(value);
    let Ok(mut url) = Url::parse(&trimmed) else {
        return trimmed;
    };
    url.set_query(None);
    url.set_fragment(None);
    if is_local_readiness_host(url.host_str().unwrap_or_default()) {
        url.set_path("/");
    }
    url.to_string().trim_end_matches('/').to_owned()
}

fn port_from_url(value: &str) -> Option<u16> {
    Url::parse(value).ok()?.port_or_known_default()
}

fn parse_local_http_url(value: &str) -> CommandResult<Url> {
    let url = Url::parse(value).map_err(|error| {
        CommandError::user_fixable(
            "autonomous_tool_process_manager_wait_url_invalid",
            format!("Xero could not parse process_manager waitUrl: {error}"),
        )
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(CommandError::user_fixable(
            "autonomous_tool_process_manager_wait_url_invalid",
            "Xero requires process_manager waitUrl to use http or https.",
        ));
    }
    let host = url.host_str().unwrap_or_default();
    if !is_local_readiness_host(host) {
        return Err(CommandError::user_fixable(
            "autonomous_tool_process_manager_wait_url_non_local",
            "Xero only probes local HTTP readiness URLs for managed processes.",
        ));
    }
    Ok(url)
}

fn is_local_readiness_host(host: &str) -> bool {
    matches!(
        host.to_ascii_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "0.0.0.0" | "::1" | "[::1]"
    )
}

fn port_is_open(port: u16) -> bool {
    [
        SocketAddr::from(([127, 0, 0, 1], port)),
        SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], port)),
    ]
    .iter()
    .any(|addr| TcpStream::connect_timeout(addr, PROCESS_MANAGER_HTTP_PROBE_TIMEOUT).is_ok())
}

fn http_url_is_ready(client: &reqwest::blocking::Client, url: Url) -> bool {
    client
        .get(url)
        .send()
        .is_ok_and(|response| response.status().is_success())
}

fn is_warning_line(line: &str) -> bool {
    let lowered = line.to_ascii_lowercase();
    lowered.contains("warning") || lowered.contains("warn:")
}

fn is_error_line(line: &str) -> bool {
    let lowered = line.to_ascii_lowercase();
    lowered.contains("error")
        || lowered.contains("failed")
        || lowered.contains("exception")
        || lowered.contains("panic")
        || lowered.contains("fatal")
}

fn is_stack_trace_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("at ")
        || trimmed.starts_with("stack backtrace:")
        || trimmed.contains("panicked at")
}

fn process_run_marker(process_id: &str, cursor: u64) -> String {
    format!(
        "{INTERNAL_MARKER_PREFIX}RUN_DONE_{}_{}__",
        marker_safe(process_id),
        cursor
    )
}

fn process_env_marker(process_id: &str, cursor: u64) -> String {
    format!(
        "{INTERNAL_MARKER_PREFIX}ENV_DONE_{}_{}__",
        marker_safe(process_id),
        cursor
    )
}

fn marker_safe(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn shell_run_payload(input: &str, marker: &str) -> String {
    format!(
        "{}\n__xero_status=$?\nprintf '\\n{}:%s\\n' \"$__xero_status\"\n",
        input.trim_end_matches('\n'),
        marker
    )
}

fn shell_env_payload(marker: &str) -> String {
    format!(
        "printf 'cwd:%s\\n' \"$PWD\"\nprintf 'shell:%s\\n' \"${{SHELL:-}}\"\nprintf 'path:%s\\n' \"$PATH\"\nprintf '{}\\n'\n",
        marker
    )
}

fn shell_policy_argv(input: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        vec!["cmd".into(), "/C".into(), input.into()]
    }
    #[cfg(not(windows))]
    {
        vec!["sh".into(), "-c".into(), input.into()]
    }
}

fn default_shell_argv() -> Vec<String> {
    #[cfg(windows)]
    {
        vec![env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into())]
    }
    #[cfg(not(windows))]
    {
        vec![env::var("SHELL")
            .ok()
            .filter(|shell| {
                let name = std::path::Path::new(shell)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or(shell)
                    .to_ascii_lowercase();
                matches!(name.as_str(), "sh" | "bash" | "zsh" | "dash" | "ksh")
            })
            .unwrap_or_else(|| "/bin/sh".into())]
    }
}

fn clean_optional_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn sanitized_env_summary() -> Vec<String> {
    let mut keys = SAFE_COMMAND_ENV_KEYS
        .iter()
        .filter(|key| env::var_os(key).is_some())
        .map(|key| (*key).to_owned())
        .collect::<Vec<_>>();
    if env::var_os("PATH").is_none() {
        keys.push("PATH".into());
    }
    keys.push("XERO_AGENT_SANITIZED_ENV".into());
    keys.sort();
    keys.dedup();
    keys
}

fn default_process_output_read_bytes() -> usize {
    PROCESS_OUTPUT_EXCERPT_BYTES
}

fn process_policy_from_command(
    command_policy: AutonomousCommandPolicyTrace,
) -> AutonomousProcessManagerPolicyTrace {
    AutonomousProcessManagerPolicyTrace {
        risk_level: AutonomousProcessActionRiskLevel::RunOwned,
        approval_required: command_policy.outcome != AutonomousCommandPolicyOutcome::Allowed,
        code: command_policy.code,
        reason: command_policy.reason,
    }
}

fn process_policy_requiring_command_approval(
    command_policy: AutonomousCommandPolicyTrace,
) -> AutonomousProcessManagerPolicyTrace {
    AutonomousProcessManagerPolicyTrace {
        risk_level: AutonomousProcessActionRiskLevel::RunOwned,
        approval_required: true,
        code: command_policy.code,
        reason: command_policy.reason,
    }
}

fn process_interaction_policy_allowed(
    action: AutonomousProcessManagerAction,
    target_ownership: Option<AutonomousProcessOwnershipScope>,
) -> AutonomousProcessManagerPolicyTrace {
    let mut policy = process_manager_policy_trace(action, target_ownership, false);
    policy.approval_required = false;
    policy.code = "process_policy_owned_interaction_allowed".into();
    policy.reason =
        "Interacting with a Xero-owned process is allowed after ownership verification and shell-input policy checks.".into();
    policy
}

fn external_signal_policy_allowed(
    action: AutonomousProcessManagerAction,
    signal_label: &str,
) -> AutonomousProcessManagerPolicyTrace {
    let mut policy = process_manager_policy_trace(
        action,
        Some(AutonomousProcessOwnershipScope::External),
        false,
    );
    policy.approval_required = false;
    policy.code = "process_policy_external_signal_allowed_after_operator_approval".into();
    policy.reason = format!(
        "Operator approval allowed Xero to apply external process signal action `{}` with signal {signal_label}.",
        process_manager_action_label(action)
    );
    policy
}

fn operator_approved_command_policy(
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

fn operator_approved_shell_policy(
    mut policy: AutonomousCommandPolicyTrace,
    argv: &[String],
) -> AutonomousCommandPolicyTrace {
    policy.outcome = AutonomousCommandPolicyOutcome::Allowed;
    policy.code = "policy_allowed_shell_after_operator_approval".into();
    policy.reason = format!(
        "Operator approval allowed interactive shell process `{}` to start.",
        render_command_for_summary(argv)
    );
    policy
}

fn shell_mode_requires_operator_policy(
    mut policy: AutonomousCommandPolicyTrace,
    argv: &[String],
) -> AutonomousCommandPolicyTrace {
    policy.outcome = AutonomousCommandPolicyOutcome::Escalated;
    policy.code = "policy_escalated_interactive_shell".into();
    policy.reason = format!(
        "Xero requires operator review before starting interactive shell process `{}`.",
        render_command_for_summary(argv)
    );
    policy
}

fn render_command_for_summary(argv: &[String]) -> String {
    render_command_for_persistence(argv)
}

fn unstarted_process_metadata(
    argv: &[String],
    cwd: String,
    shell_mode: bool,
    label: Option<String>,
    process_type: Option<String>,
    group: Option<String>,
) -> AutonomousProcessMetadata {
    AutonomousProcessMetadata {
        process_id: "unstarted".into(),
        pid: None,
        parent_pid: None,
        process_group_id: None,
        process_name: argv.first().cloned(),
        executable_path: None,
        label: clean_optional_string(label.as_deref()),
        process_type: clean_optional_string(process_type.as_deref()),
        group: clean_optional_string(group.as_deref()),
        owner: AutonomousProcessOwner {
            thread_id: None,
            session_id: None,
            repo_id: None,
            user_id: None,
            scope: AutonomousProcessOwnershipScope::XeroOwned,
        },
        command: AutonomousProcessCommandMetadata {
            argv: redact_command_argv_for_persistence(argv),
            shell_mode,
            cwd,
            sanitized_env: sanitized_env_summary(),
        },
        stdin_state: AutonomousProcessStdinState::Unavailable,
        status: AutonomousProcessStatus::Starting,
        started_at: None,
        exited_at: None,
        exit_code: None,
        output_cursor: Some(0),
        detected_urls: Vec::new(),
        detected_ports: Vec::new(),
        recent_errors: Vec::new(),
        recent_warnings: Vec::new(),
        recent_stack_traces: Vec::new(),
        status_changes: Vec::new(),
        readiness: AutonomousProcessReadinessState {
            ready: false,
            detector: None,
            matched: None,
        },
        restart_count: 0,
        last_restart_reason: None,
        async_job: false,
        timeout_ms: None,
        output_artifact: None,
    }
}

fn prune_process_output_chunks(chunks: &mut Vec<AutonomousProcessOutputChunk>) {
    let mut total_bytes = chunks.iter().map(process_output_chunk_bytes).sum::<usize>();
    let mut drop_count = 0;
    while chunks.len().saturating_sub(drop_count) > RECENT_OUTPUT_RING_CHUNKS
        || total_bytes > RECENT_OUTPUT_RING_BYTES
    {
        let Some(chunk) = chunks.get(drop_count) else {
            break;
        };
        total_bytes = total_bytes.saturating_sub(process_output_chunk_bytes(chunk));
        drop_count += 1;
    }

    if drop_count > 0 {
        chunks.drain(0..drop_count);
    }
}

fn process_output_chunk_bytes(chunk: &AutonomousProcessOutputChunk) -> usize {
    chunk.text.as_deref().map(str::len).unwrap_or_default()
}

fn prune_raw_process_output_chunks(chunks: &mut Vec<RawProcessOutputChunk>) {
    let mut total_bytes = chunks.iter().map(|chunk| chunk.text.len()).sum::<usize>();
    let mut drop_count = 0;
    while chunks.len().saturating_sub(drop_count) > RECENT_OUTPUT_RING_CHUNKS
        || total_bytes > RECENT_OUTPUT_RING_BYTES
    {
        let Some(chunk) = chunks.get(drop_count) else {
            break;
        };
        total_bytes = total_bytes.saturating_sub(chunk.text.len());
        drop_count += 1;
    }

    if drop_count > 0 {
        chunks.drain(0..drop_count);
    }
}

fn filter_internal_marker_chunk(
    mut chunk: AutonomousProcessOutputChunk,
) -> AutonomousProcessOutputChunk {
    let Some(text) = chunk.text.as_deref() else {
        return chunk;
    };
    if !text.contains(INTERNAL_MARKER_PREFIX) {
        return chunk;
    }

    let filtered = text
        .lines()
        .filter(|line| !line.contains(INTERNAL_MARKER_PREFIX))
        .collect::<Vec<_>>()
        .join("\n");
    chunk.text = if filtered.trim().is_empty() {
        None
    } else {
        Some(filtered)
    };
    chunk
}

fn filter_internal_marker_text(text: &str) -> String {
    if !text.contains(INTERNAL_MARKER_PREFIX) {
        return text.to_owned();
    }

    text.lines()
        .filter(|line| !line.contains(INTERNAL_MARKER_PREFIX))
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug)]
struct SanitizedProcessOutput {
    text: Option<String>,
    truncated: bool,
    redacted: bool,
}

fn sanitize_process_output(bytes: &[u8], truncated: bool) -> SanitizedProcessOutput {
    if bytes.is_empty() {
        return SanitizedProcessOutput {
            text: None,
            truncated,
            redacted: false,
        };
    }

    let decoded = String::from_utf8_lossy(bytes).into_owned();
    if decoded.contains('\0')
        || decoded
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return SanitizedProcessOutput {
            text: Some(REDACTED_PROCESS_OUTPUT_SUMMARY.into()),
            truncated,
            redacted: true,
        };
    }

    let collapsed = decoded.replace("\r\n", "\n").replace('\r', "\n");
    let mut redacted = false;
    let mut sanitized_lines = Vec::new();
    for line in collapsed.lines() {
        if find_prohibited_persistence_content(line).is_some() {
            redacted = true;
            if sanitized_lines
                .last()
                .is_none_or(|last| *last != REDACTED_PROCESS_OUTPUT_SUMMARY)
            {
                sanitized_lines.push(REDACTED_PROCESS_OUTPUT_SUMMARY);
            }
        } else {
            sanitized_lines.push(line);
        }
    }
    let sanitized = sanitized_lines.join("\n");
    let trimmed = sanitized.trim();
    if trimmed.is_empty() {
        return SanitizedProcessOutput {
            text: None,
            truncated,
            redacted,
        };
    }

    SanitizedProcessOutput {
        text: Some(truncate_chars(trimmed, PROCESS_OUTPUT_EXCERPT_BYTES)),
        truncated,
        redacted,
    }
}

fn decode_process_output_for_intelligence(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    let decoded = String::from_utf8_lossy(bytes).into_owned();
    if decoded.contains('\0')
        || decoded
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return None;
    }
    let collapsed = decoded.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(truncate_chars(trimmed, PROCESS_OUTPUT_EXCERPT_BYTES))
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

fn spawn_owned_process_reader(
    process: Arc<OwnedProcess>,
    mut reader: impl Read + Send + 'static,
    stream: AutonomousProcessOutputStream,
) {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let raw_text = decode_process_output_for_intelligence(&buffer[..read]);
                    let capture = sanitize_process_output(&buffer[..read], false);
                    let _ = process.push_chunk(stream, capture, raw_text);
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    let capture = SanitizedProcessOutput {
                        text: Some(format!("Owned process output read failed: {error}")),
                        truncated: false,
                        redacted: false,
                    };
                    let _ = process.push_chunk(stream, capture, None);
                    break;
                }
            }
        }
    });
}

fn spawn_async_job_timeout_monitor(process: Arc<OwnedProcess>, timeout: Duration) {
    thread::spawn(move || {
        let started = Instant::now();
        loop {
            match process.poll_exit() {
                Ok(Some(_)) => return,
                Ok(None) => {}
                Err(_) => return,
            }
            if started.elapsed() >= timeout {
                let capture = SanitizedProcessOutput {
                    text: Some(format!(
                        "Async job timed out after {} ms.",
                        timeout.as_millis()
                    )),
                    truncated: false,
                    redacted: false,
                };
                let _ = process.push_chunk(AutonomousProcessOutputStream::Stderr, capture, None);
                let _ = process.kill();
                return;
            }
            thread::sleep(PROCESS_MANAGER_WAIT_POLL);
        }
    });
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::fs;
    use std::{thread, time::Duration};

    use super::*;
    use crate::{
        commands::{
            RuntimeAgentIdDto, RuntimeRunActiveControlSnapshotDto, RuntimeRunApprovalModeDto,
            RuntimeRunControlStateDto,
        },
        runtime::AutonomousToolRequest,
    };

    #[test]
    fn returned_output_cursor_advances_only_contiguous_returned_chunks() {
        let selected = vec![
            test_chunk(1, AutonomousProcessOutputStream::Stdout, "alpha"),
            test_chunk(2, AutonomousProcessOutputStream::Stderr, "warning"),
            test_chunk(3, AutonomousProcessOutputStream::Stdout, "omega"),
        ];

        assert_eq!(
            returned_output_advance_cursor(0, &selected, &selected[..1]),
            Some(1)
        );
        assert_eq!(
            returned_output_advance_cursor(0, &selected, &selected[1..2]),
            None
        );
        assert_eq!(
            returned_output_advance_cursor(
                0,
                &selected,
                &[selected[0].clone(), selected[2].clone()]
            ),
            Some(1)
        );
    }

    #[test]
    fn returned_output_cursor_does_not_advance_for_tail_synthetic_chunk() {
        let selected = vec![
            test_chunk(1, AutonomousProcessOutputStream::Stdout, "alpha"),
            test_chunk(2, AutonomousProcessOutputStream::Stdout, "beta\ngamma"),
        ];
        let tailed = tail_process_output_chunks(selected.clone(), 1);

        assert_eq!(returned_output_advance_cursor(0, &selected, &tailed), None);
    }

    #[test]
    fn owned_process_can_start_output_list_status_and_kill() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path());

        let start = runtime
            .execute(AutonomousToolRequest::ProcessManager(start_request(
                long_running_output_command(),
            )))
            .expect("start process");
        let start_output = process_manager_output(start);
        assert!(start_output.spawned);
        let process_id = start_output.process_id.clone().expect("process id");

        let output = wait_for_process_output(&runtime, &process_id, "ready");
        assert!(
            output
                .chunks
                .iter()
                .filter_map(|chunk| chunk.text.as_deref())
                .any(|text| text.contains("ready")),
            "expected ready output chunk"
        );

        let list = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(base_request(
                    AutonomousProcessManagerAction::List,
                )))
                .expect("list processes"),
        );
        assert!(
            list.processes
                .iter()
                .any(|process| process.process_id == process_id),
            "started process should appear in list"
        );

        let mut status_request = base_request(AutonomousProcessManagerAction::Status);
        status_request.process_id = Some(process_id.clone());
        let status = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(status_request))
                .expect("status process"),
        );
        assert_eq!(status.processes[0].process_id, process_id);
        assert!(matches!(
            status.processes[0].status,
            AutonomousProcessStatus::Running | AutonomousProcessStatus::Exited
        ));

        let mut kill_request = base_request(AutonomousProcessManagerAction::Kill);
        kill_request.process_id = Some(process_id.clone());
        let kill = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(kill_request))
                .expect("kill process"),
        );
        assert_eq!(kill.processes[0].process_id, process_id);
        assert!(matches!(
            kill.processes[0].status,
            AutonomousProcessStatus::Killed | AutonomousProcessStatus::Exited
        ));

        let list_after_kill = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(base_request(
                    AutonomousProcessManagerAction::List,
                )))
                .expect("list after kill"),
        );
        assert!(
            list_after_kill
                .processes
                .iter()
                .all(|process| process.process_id != process_id),
            "killed process should be removed from registry"
        );
    }

    #[cfg(unix)]
    #[test]
    fn killing_owned_process_terminates_child_tree() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path());
        let pid_file = tempdir.path().join("child.pid");
        let command = vec![
            "sh".into(),
            "-c".into(),
            "sleep 30 & echo $! > child.pid; wait".into(),
        ];

        let start = runtime
            .execute_approved(AutonomousToolRequest::ProcessManager(start_request(
                command,
            )))
            .expect("start process tree");
        let start_output = process_manager_output(start);
        let process_id = start_output.process_id.clone().expect("process id");
        let child_pid = wait_for_child_pid(&pid_file);
        assert!(
            unix_process_exists(child_pid),
            "child process should be alive"
        );

        let mut kill_request = base_request(AutonomousProcessManagerAction::Kill);
        kill_request.process_id = Some(process_id);
        let _ = runtime
            .execute(AutonomousToolRequest::ProcessManager(kill_request))
            .expect("kill process tree");

        for _ in 0..20 {
            if !unix_process_exists(child_pid) {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!("child process {child_pid} survived process-manager kill");
    }

    #[cfg(unix)]
    #[test]
    fn interactive_owned_process_can_answer_prompt() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path());
        let mut start_request = start_request(vec![
            "sh".into(),
            "-c".into(),
            "printf 'name? '; read name; printf 'hello %s\\n' \"$name\"; sleep 30".into(),
        ]);
        start_request.interactive = true;

        let start = process_manager_output(
            runtime
                .execute_approved(AutonomousToolRequest::ProcessManager(start_request))
                .expect("start interactive process"),
        );
        let process_id = start.process_id.expect("process id");
        let prompt = wait_for_process_output(&runtime, &process_id, "name?");
        assert_eq!(
            prompt.processes[0].stdin_state,
            AutonomousProcessStdinState::Open
        );

        let mut send_request = base_request(AutonomousProcessManagerAction::SendAndWait);
        send_request.process_id = Some(process_id.clone());
        send_request.input = Some("Ada\n".into());
        send_request.wait_pattern = Some("hello Ada".into());
        let send = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(send_request))
                .expect("send prompt answer"),
        );
        assert!(
            output_contains(&send, "hello Ada"),
            "expected prompt response in output chunks"
        );

        kill_process(&runtime, process_id);
    }

    #[cfg(unix)]
    #[test]
    fn send_and_wait_timeout_leaves_process_running() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path());
        let mut start_request = start_request(vec![
            "sh".into(),
            "-c".into(),
            "while read line; do printf 'got:%s\\n' \"$line\"; done".into(),
        ]);
        start_request.interactive = true;

        let start = process_manager_output(
            runtime
                .execute_approved(AutonomousToolRequest::ProcessManager(start_request))
                .expect("start interactive process"),
        );
        let process_id = start.process_id.expect("process id");

        let mut send_request = base_request(AutonomousProcessManagerAction::SendAndWait);
        send_request.process_id = Some(process_id.clone());
        send_request.input = Some("ping\n".into());
        send_request.wait_pattern = Some("never-matches".into());
        send_request.timeout_ms = Some(100);
        let send = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(send_request))
                .expect("send with timeout"),
        );
        assert!(
            send.message.contains("timed out"),
            "timeout should be reported without failing the tool"
        );

        let mut status_request = base_request(AutonomousProcessManagerAction::Status);
        status_request.process_id = Some(process_id.clone());
        let status = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(status_request))
                .expect("status after send timeout"),
        );
        assert_eq!(status.processes[0].status, AutonomousProcessStatus::Running);

        kill_process(&runtime, process_id);
    }

    #[cfg(unix)]
    #[test]
    fn shell_run_preserves_cwd_and_env_reports_it() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(tempdir.path().join("nested")).expect("create nested dir");
        let runtime = test_runtime(tempdir.path());
        let mut start_request = base_request(AutonomousProcessManagerAction::Start);
        start_request.shell_mode = true;
        start_request.argv = vec!["sh".into()];

        let start = process_manager_output(
            runtime
                .execute_approved(AutonomousToolRequest::ProcessManager(start_request))
                .expect("start shell process"),
        );
        let process_id = start.process_id.expect("process id");

        let mut cd_request = base_request(AutonomousProcessManagerAction::Run);
        cd_request.process_id = Some(process_id.clone());
        cd_request.input = Some("cd nested".into());
        let cd = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(cd_request))
                .expect("run cd"),
        );
        assert!(cd.spawned);

        let mut pwd_request = base_request(AutonomousProcessManagerAction::Run);
        pwd_request.process_id = Some(process_id.clone());
        pwd_request.input = Some("pwd".into());
        let pwd = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(pwd_request))
                .expect("run pwd"),
        );
        assert!(
            output_contains(&pwd, "nested"),
            "shell cwd should persist between run calls"
        );

        let mut env_request = base_request(AutonomousProcessManagerAction::Env);
        env_request.process_id = Some(process_id.clone());
        let env = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(env_request))
                .expect("read shell env"),
        );
        assert!(output_contains(&env, "cwd:"));
        assert!(output_contains(&env, "nested"));

        kill_process(&runtime, process_id);
    }

    #[cfg(unix)]
    #[test]
    fn shell_run_destructive_input_requires_approval_without_writing() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path());
        let mut start_request = base_request(AutonomousProcessManagerAction::Start);
        start_request.shell_mode = true;
        start_request.argv = vec!["sh".into()];

        let start = process_manager_output(
            runtime
                .execute_approved(AutonomousToolRequest::ProcessManager(start_request))
                .expect("start shell process"),
        );
        let process_id = start.process_id.expect("process id");

        let mut run_request = base_request(AutonomousProcessManagerAction::Run);
        run_request.process_id = Some(process_id.clone());
        run_request.input = Some("rm -rf .".into());
        let run = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(run_request))
                .expect("destructive shell input should become action-required output"),
        );
        assert!(!run.spawned);
        assert!(run.policy.approval_required);

        let mut status_request = base_request(AutonomousProcessManagerAction::Status);
        status_request.process_id = Some(process_id.clone());
        let status = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(status_request))
                .expect("status after blocked run"),
        );
        assert_eq!(status.processes[0].status, AutonomousProcessStatus::Running);

        kill_process(&runtime, process_id);
    }

    #[cfg(unix)]
    #[test]
    fn readiness_digest_highlights_and_since_last_read_summarize_owned_processes() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path());
        let mut start_request = start_request(vec![
            "sh".into(),
            "-c".into(),
            "printf 'Server ready at http://127.0.0.1:4321\\nwarning: beta path\\nerror: sample failure\\n    at app.js:1\\n'; sleep 30".into(),
        ]);
        start_request.label = Some("dev server".into());

        let start = process_manager_output(
            runtime
                .execute_approved(AutonomousToolRequest::ProcessManager(start_request))
                .expect("start highlighted process"),
        );
        let process_id = start.process_id.expect("process id");

        let mut ready_request = base_request(AutonomousProcessManagerAction::WaitForReady);
        ready_request.process_id = Some(process_id.clone());
        ready_request.wait_pattern = Some("Server ready".into());
        let ready = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(ready_request))
                .expect("wait for output readiness"),
        );
        assert!(ready.processes[0].readiness.ready);
        assert_eq!(
            ready.processes[0].readiness.detector,
            Some(AutonomousProcessReadinessDetector::OutputRegex)
        );
        assert!(ready.processes[0]
            .detected_urls
            .contains(&"http://127.0.0.1:4321".into()));
        assert!(ready.processes[0].detected_ports.contains(&4321));
        assert!(ready.processes[0]
            .recent_warnings
            .iter()
            .any(|warning| warning.contains("warning: beta")));
        assert!(ready.processes[0]
            .recent_errors
            .iter()
            .any(|error| error.contains("error: sample")));
        assert!(ready.processes[0]
            .recent_stack_traces
            .iter()
            .any(|stack| stack.contains("app.js")));

        let mut output_request = base_request(AutonomousProcessManagerAction::Output);
        output_request.process_id = Some(process_id.clone());
        let output = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(output_request))
                .expect("initial output read"),
        );
        assert!(output_contains(&output, "warning: beta"));

        let mut since_request = base_request(AutonomousProcessManagerAction::Output);
        since_request.process_id = Some(process_id.clone());
        since_request.since_last_read = true;
        let since = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(since_request))
                .expect("since-last-read output"),
        );
        assert!(
            since.chunks.is_empty(),
            "since-last-read should not replay output already read"
        );

        let mut highlights_request = base_request(AutonomousProcessManagerAction::Highlights);
        highlights_request.process_id = Some(process_id.clone());
        let highlights = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(highlights_request))
                .expect("highlights"),
        );
        assert!(highlights
            .highlights
            .iter()
            .any(|highlight| highlight.kind == AutonomousProcessHighlightKind::Url));
        assert!(highlights
            .highlights
            .iter()
            .any(|highlight| highlight.kind == AutonomousProcessHighlightKind::Error));

        let digest = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(base_request(
                    AutonomousProcessManagerAction::Digest,
                )))
                .expect("digest"),
        );
        assert!(digest
            .digest
            .as_deref()
            .is_some_and(|value| value.contains("dev server") && value.contains("ready")));

        kill_process(&runtime, process_id);
    }

    #[cfg(unix)]
    #[test]
    fn wait_for_ready_supports_local_port_probe() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind local listener");
        let port = listener.local_addr().expect("listener addr").port();
        let listener_thread = thread::spawn(move || {
            let _ = listener.accept();
        });
        let start = process_manager_output(
            runtime
                .execute_approved(AutonomousToolRequest::ProcessManager(start_request(
                    long_running_output_command(),
                )))
                .expect("start long-running process"),
        );
        let process_id = start.process_id.expect("process id");

        let mut ready_request = base_request(AutonomousProcessManagerAction::WaitForReady);
        ready_request.process_id = Some(process_id.clone());
        ready_request.wait_port = Some(port);
        ready_request.timeout_ms = Some(5_000);
        let ready = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(ready_request))
                .expect("wait for port readiness"),
        );
        assert!(ready.processes[0].readiness.ready);
        assert_eq!(
            ready.processes[0].readiness.detector,
            Some(AutonomousProcessReadinessDetector::PortOpen)
        );

        kill_process(&runtime, process_id);
        listener_thread.join().expect("listener thread");
    }

    #[cfg(unix)]
    #[test]
    fn restart_and_group_actions_control_related_owned_processes() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path());

        let mut first_request = start_request(vec![
            "sh".into(),
            "-c".into(),
            "printf 'ready-a\\n'; sleep 30".into(),
        ]);
        first_request.group = Some("dev".into());
        first_request.label = Some("server a".into());
        let first = process_manager_output(
            runtime
                .execute_approved(AutonomousToolRequest::ProcessManager(first_request))
                .expect("start first process"),
        );
        let first_id = first.process_id.expect("first process id");

        let mut second_request = start_request(vec![
            "sh".into(),
            "-c".into(),
            "printf 'ready-b\\n'; sleep 30".into(),
        ]);
        second_request.group = Some("dev".into());
        second_request.label = Some("server b".into());
        let second = process_manager_output(
            runtime
                .execute_approved(AutonomousToolRequest::ProcessManager(second_request))
                .expect("start second process"),
        );
        let second_id = second.process_id.expect("second process id");

        let mut restart_request = base_request(AutonomousProcessManagerAction::Restart);
        restart_request.process_id = Some(first_id.clone());
        restart_request.input = Some("refresh after config change".into());
        let restarted = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(restart_request))
                .expect("restart first process"),
        );
        assert_eq!(restarted.process_id.as_deref(), Some(first_id.as_str()));
        assert_eq!(restarted.processes[0].restart_count, 1);
        assert_eq!(
            restarted.processes[0].last_restart_reason.as_deref(),
            Some("refresh after config change")
        );
        assert_eq!(restarted.processes[0].group.as_deref(), Some("dev"));

        let mut group_status_request = base_request(AutonomousProcessManagerAction::GroupStatus);
        group_status_request.group = Some("dev".into());
        let group_status = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(group_status_request))
                .expect("group status"),
        );
        assert_eq!(group_status.processes.len(), 2);
        assert!(group_status
            .processes
            .iter()
            .any(|process| process.process_id == first_id));
        assert!(group_status
            .processes
            .iter()
            .any(|process| process.process_id == second_id));
        assert!(group_status
            .digest
            .as_deref()
            .is_some_and(|digest| digest.contains("server a") && digest.contains("server b")));

        let mut group_kill_request = base_request(AutonomousProcessManagerAction::GroupKill);
        group_kill_request.group = Some("dev".into());
        let group_kill = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(group_kill_request))
                .expect("group kill"),
        );
        assert_eq!(group_kill.processes.len(), 2);
        assert!(group_kill.processes.iter().all(|process| matches!(
            process.status,
            AutonomousProcessStatus::Killed | AutonomousProcessStatus::Exited
        )));

        let list_after_kill = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(base_request(
                    AutonomousProcessManagerAction::List,
                )))
                .expect("list after group kill"),
        );
        assert!(list_after_kill
            .processes
            .iter()
            .all(|process| process.group.as_deref() != Some("dev")));
    }

    #[cfg(unix)]
    #[test]
    fn async_jobs_can_be_awaited_and_cancelled() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path());

        let mut async_request = start_request(vec![
            "sh".into(),
            "-c".into(),
            "printf 'job done\\n'".into(),
        ]);
        async_request.action = AutonomousProcessManagerAction::AsyncStart;
        async_request.group = Some("jobs".into());
        async_request.timeout_ms = Some(5_000);
        let async_start = process_manager_output(
            runtime
                .execute_approved(AutonomousToolRequest::ProcessManager(async_request))
                .expect("start async job"),
        );
        assert_eq!(
            async_start.action,
            AutonomousProcessManagerAction::AsyncStart
        );
        assert!(async_start.processes[0].async_job);
        assert_eq!(async_start.processes[0].timeout_ms, Some(5_000));
        let async_job_id = async_start.process_id.clone().expect("async job id");

        let mut await_request = base_request(AutonomousProcessManagerAction::AsyncAwait);
        await_request.group = Some("jobs".into());
        await_request.timeout_ms = Some(5_000);
        let awaited = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(await_request))
                .expect("await any async job"),
        );
        assert_eq!(awaited.action, AutonomousProcessManagerAction::AsyncAwait);
        assert!(output_contains(&awaited, "job done"));
        let artifact = awaited.processes[0]
            .output_artifact
            .as_ref()
            .expect("async job output artifact");
        let artifact_path = std::path::Path::new(&artifact.path);
        let expected_artifact_dir =
            crate::db::project_app_data_dir_for_repo(tempdir.path()).join(ASYNC_JOB_ARTIFACT_DIR);
        assert!(
            artifact_path.starts_with(&expected_artifact_dir),
            "async job artifact should live under project app-data: {}",
            artifact.path
        );
        assert!(artifact_path.is_file());
        assert!(artifact.byte_count > 0);
        assert!(
            std::fs::read_to_string(&artifact.path)
                .expect("read async artifact")
                .contains("job done"),
            "async job artifact should contain the completed output"
        );

        let list_after_await = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(base_request(
                    AutonomousProcessManagerAction::List,
                )))
                .expect("list after async await"),
        );
        assert!(list_after_await
            .processes
            .iter()
            .all(|process| process.process_id != async_job_id));

        let mut cancellable_request = start_request(vec![
            "sh".into(),
            "-c".into(),
            "printf 'still running\\n'; sleep 30".into(),
        ]);
        cancellable_request.action = AutonomousProcessManagerAction::AsyncStart;
        cancellable_request.group = Some("jobs".into());
        cancellable_request.timeout_ms = Some(5_000);
        let cancellable = process_manager_output(
            runtime
                .execute_approved(AutonomousToolRequest::ProcessManager(cancellable_request))
                .expect("start cancellable async job"),
        );
        let cancellable_id = cancellable.process_id.expect("cancellable id");

        let mut cancel_request = base_request(AutonomousProcessManagerAction::AsyncCancel);
        cancel_request.process_id = Some(cancellable_id.clone());
        let cancelled = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(cancel_request))
                .expect("cancel async job"),
        );
        assert_eq!(
            cancelled.process_id.as_deref(),
            Some(cancellable_id.as_str())
        );
        assert!(matches!(
            cancelled.processes[0].status,
            AutonomousProcessStatus::Killed | AutonomousProcessStatus::Exited
        ));

        let list_after_cancel = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(base_request(
                    AutonomousProcessManagerAction::List,
                )))
                .expect("list after async cancel"),
        );
        assert!(list_after_cancel
            .processes
            .iter()
            .all(|process| process.process_id != cancellable_id));
    }

    #[cfg(unix)]
    #[test]
    fn async_jobs_timeout_and_runtime_shutdown_are_finalized() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path());

        let mut timeout_request = start_request(vec![
            "sh".into(),
            "-c".into(),
            "printf 'timeout job\\n'; sleep 30".into(),
        ]);
        timeout_request.action = AutonomousProcessManagerAction::AsyncStart;
        timeout_request.group = Some("jobs".into());
        timeout_request.timeout_ms = Some(50);
        let timeout_start = process_manager_output(
            runtime
                .execute_approved(AutonomousToolRequest::ProcessManager(timeout_request))
                .expect("start timed async job"),
        );
        let timeout_job_id = timeout_start.process_id.clone().expect("timeout job id");

        let mut await_request = base_request(AutonomousProcessManagerAction::AsyncAwait);
        await_request.process_id = Some(timeout_job_id.clone());
        await_request.timeout_ms = Some(5_000);
        let awaited = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(await_request))
                .expect("await timed async job"),
        );
        assert_eq!(awaited.process_id.as_deref(), Some(timeout_job_id.as_str()));
        assert!(matches!(
            awaited.processes[0].status,
            AutonomousProcessStatus::Killed | AutonomousProcessStatus::Exited
        ));
        assert!(output_contains(&awaited, "Async job timed out"));

        let list_after_timeout = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(base_request(
                    AutonomousProcessManagerAction::List,
                )))
                .expect("list after timed async job"),
        );
        assert!(list_after_timeout
            .processes
            .iter()
            .all(|process| process.process_id != timeout_job_id));

        let shutdown_pid = {
            let runtime = test_runtime(tempdir.path());
            let mut shutdown_request = start_request(vec![
                "sh".into(),
                "-c".into(),
                "printf 'shutdown job\\n'; sleep 30".into(),
            ]);
            shutdown_request.action = AutonomousProcessManagerAction::AsyncStart;
            shutdown_request.group = Some("shutdown".into());
            shutdown_request.timeout_ms = Some(5_000);
            let shutdown_start = process_manager_output(
                runtime
                    .execute_approved(AutonomousToolRequest::ProcessManager(shutdown_request))
                    .expect("start shutdown async job"),
            );
            let pid = shutdown_start.processes[0].pid.expect("shutdown job pid") as i32;
            assert!(unix_process_exists(pid), "shutdown job should be alive");
            pid
        };

        for _ in 0..20 {
            if !unix_process_exists(shutdown_pid) {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!("async job {shutdown_pid} survived runtime shutdown");
    }

    #[cfg(unix)]
    #[test]
    fn system_process_list_and_tree_observe_external_processes() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path());
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn external sleep");
        let pid = child.id();

        let mut list_request = base_request(AutonomousProcessManagerAction::SystemProcessList);
        list_request.pid = Some(pid);
        let listed = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(list_request))
                .expect("list external process"),
        );
        assert_eq!(
            listed.action,
            AutonomousProcessManagerAction::SystemProcessList
        );
        assert_eq!(listed.processes[0].pid, Some(pid));
        assert_eq!(
            listed.processes[0].owner.scope,
            AutonomousProcessOwnershipScope::External
        );

        let mut tree_request = base_request(AutonomousProcessManagerAction::SystemProcessTree);
        tree_request.pid = Some(pid);
        let tree = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(tree_request))
                .expect("inspect external tree"),
        );
        assert!(tree
            .processes
            .iter()
            .any(|process| process.pid == Some(pid)));
        assert!(!tree.policy.approval_required);

        let _ = child.kill();
        let _ = child.wait();
    }

    #[cfg(unix)]
    #[test]
    fn system_kill_tree_requires_approval_then_kills_external_process() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path());
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn external sleep");
        let pid = child.id();

        let mut blocked_request = base_request(AutonomousProcessManagerAction::SystemKillTree);
        blocked_request.pid = Some(pid);
        let blocked = process_manager_output(
            runtime
                .execute(AutonomousToolRequest::ProcessManager(blocked_request))
                .expect("blocked external kill"),
        );
        assert!(!blocked.spawned);
        assert!(blocked.policy.approval_required);
        assert!(
            process_exists(pid),
            "blocked kill must not terminate target"
        );

        let mut approved_request = base_request(AutonomousProcessManagerAction::SystemKillTree);
        approved_request.pid = Some(pid);
        let approved = process_manager_output(
            runtime
                .execute_approved(AutonomousToolRequest::ProcessManager(approved_request))
                .expect("approved external kill"),
        );
        assert!(approved.spawned);
        for _ in 0..20 {
            if child.try_wait().expect("observe killed child").is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("external process {pid} survived approved system_kill_tree");
    }

    #[cfg(unix)]
    #[test]
    fn system_port_list_identifies_local_listener() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind local listener");
        let port = listener.local_addr().expect("listener addr").port();

        for _ in 0..10 {
            let mut request = base_request(AutonomousProcessManagerAction::SystemPortList);
            request.port = Some(port);
            let output = process_manager_output(
                runtime
                    .execute(AutonomousToolRequest::ProcessManager(request))
                    .expect("list system ports"),
            );
            if output
                .system_ports
                .iter()
                .any(|entry| entry.local_port == port)
            {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!("system_port_list did not report listener on port {port}");
    }

    #[test]
    fn windows_process_and_port_parsers_accept_fixture_shapes() {
        let processes = parse_windows_process_json(
            r#"[
              {"ProcessId":4321,"ParentProcessId":4000,"Name":"node.exe","ExecutablePath":"C:\\Program Files\\nodejs\\node.exe","CommandLine":"\"C:\\Program Files\\nodejs\\node.exe\" server.js"},
              {"ProcessId":9876,"ParentProcessId":4321,"Name":"xero.exe","ExecutablePath":null,"CommandLine":null}
            ]"#,
        )
        .expect("parse powershell process json");
        assert_eq!(processes.len(), 2);
        assert_eq!(processes[0].pid, 4321);
        assert_eq!(processes[0].parent_pid, Some(4000));
        assert_eq!(processes[0].name, "node.exe");
        assert!(processes[0].argv[0].contains("server.js"));
        assert_eq!(processes[1].argv, vec!["xero.exe".to_string()]);

        let tasklist = parse_windows_tasklist_csv(
            "\"Image Name\",\"PID\",\"Session Name\",\"Session#\",\"Mem Usage\",\"Status\",\"User Name\",\"CPU Time\",\"Window Title\"\n\
             \"node.exe\",\"4321\",\"Console\",\"1\",\"12,340 K\",\"Running\",\"DEV\\\\alice\",\"0:00:01\",\"N/A\"\n",
        )
        .expect("parse tasklist csv");
        assert_eq!(tasklist.len(), 1);
        assert_eq!(tasklist[0].pid, 4321);
        assert_eq!(tasklist[0].name, "node.exe");

        let ports = parse_windows_net_tcp_json(
            r#"[
              {"LocalAddress":"127.0.0.1","LocalPort":3000,"State":"Listen","OwningProcess":4321},
              {"LocalAddress":"::","LocalPort":"5173","State":"Listen","OwningProcess":"9876"}
            ]"#,
        )
        .expect("parse Get-NetTCPConnection json");
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].local_addr, "127.0.0.1");
        assert_eq!(ports[0].local_port, 3000);
        assert_eq!(ports[0].pid, Some(4321));
        assert_eq!(ports[1].protocol, "tcp6");
        assert_eq!(ports[1].local_port, 5173);

        let netstat = parse_windows_netstat(
            "\n  Proto  Local Address          Foreign Address        State           PID\n\
               TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       1000\n\
               TCP    [::]:3000              [::]:0                 LISTENING       4321\n",
        )
        .expect("parse netstat");
        assert_eq!(netstat.len(), 2);
        assert_eq!(netstat[0].local_addr, "0.0.0.0");
        assert_eq!(netstat[0].local_port, 135);
        assert_eq!(netstat[1].local_addr, "::");
        assert_eq!(netstat[1].protocol, "tcp6");
    }

    fn test_runtime(repo_root: &std::path::Path) -> AutonomousToolRuntime {
        AutonomousToolRuntime::new(repo_root)
            .expect("runtime")
            .with_runtime_run_controls(RuntimeRunControlStateDto {
                active: RuntimeRunActiveControlSnapshotDto {
                    runtime_agent_id: RuntimeAgentIdDto::Engineer,
                    agent_definition_id: None,
                    agent_definition_version: None,
                    provider_profile_id: None,
                    model_id: "test-model".into(),
                    thinking_effort: None,
                    approval_mode: RuntimeRunApprovalModeDto::Yolo,
                    plan_mode_required: false,
                    revision: 1,
                    applied_at: now_timestamp(),
                },
                pending: None,
            })
    }

    fn long_running_output_command() -> Vec<String> {
        #[cfg(unix)]
        {
            vec![
                "sh".into(),
                "-c".into(),
                "printf 'ready\\n'; sleep 30".into(),
            ]
        }
        #[cfg(windows)]
        {
            vec![
                "cmd".into(),
                "/C".into(),
                "echo ready && timeout /T 30 > NUL".into(),
            ]
        }
    }

    fn start_request(argv: Vec<String>) -> AutonomousProcessManagerRequest {
        let mut request = base_request(AutonomousProcessManagerAction::Start);
        request.argv = argv;
        request
    }

    fn base_request(action: AutonomousProcessManagerAction) -> AutonomousProcessManagerRequest {
        AutonomousProcessManagerRequest {
            action,
            process_id: None,
            pid: None,
            parent_pid: None,
            port: None,
            group: None,
            label: None,
            process_type: None,
            argv: Vec::new(),
            cwd: None,
            shell_mode: false,
            interactive: false,
            target_ownership: None,
            persistent: false,
            timeout_ms: None,
            after_cursor: None,
            since_last_read: false,
            max_bytes: None,
            tail_lines: None,
            stream: None,
            filter: None,
            input: None,
            wait_pattern: None,
            wait_port: None,
            wait_url: None,
            signal: None,
        }
    }

    fn output_contains(output: &AutonomousProcessManagerOutput, needle: &str) -> bool {
        output
            .chunks
            .iter()
            .filter_map(|chunk| chunk.text.as_deref())
            .any(|text| text.contains(needle))
    }

    fn kill_process(runtime: &AutonomousToolRuntime, process_id: String) {
        let mut kill_request = base_request(AutonomousProcessManagerAction::Kill);
        kill_request.process_id = Some(process_id);
        let _ = runtime
            .execute(AutonomousToolRequest::ProcessManager(kill_request))
            .expect("kill process");
    }

    fn wait_for_process_output(
        runtime: &AutonomousToolRuntime,
        process_id: &str,
        needle: &str,
    ) -> AutonomousProcessManagerOutput {
        let mut last = None;
        for _ in 0..20 {
            let mut request = base_request(AutonomousProcessManagerAction::Output);
            request.process_id = Some(process_id.into());
            let output = process_manager_output(
                runtime
                    .execute(AutonomousToolRequest::ProcessManager(request))
                    .expect("read output"),
            );
            if output
                .chunks
                .iter()
                .filter_map(|chunk| chunk.text.as_deref())
                .any(|text| text.contains(needle))
            {
                return output;
            }
            last = Some(output);
            thread::sleep(Duration::from_millis(50));
        }
        last.expect("output attempts")
    }

    fn process_manager_output(result: AutonomousToolResult) -> AutonomousProcessManagerOutput {
        match result.output {
            AutonomousToolOutput::ProcessManager(output) => output,
            other => panic!("expected process manager output, got {other:?}"),
        }
    }

    fn test_chunk(
        cursor: u64,
        stream: AutonomousProcessOutputStream,
        text: &str,
    ) -> AutonomousProcessOutputChunk {
        AutonomousProcessOutputChunk {
            cursor,
            stream,
            text: Some(text.into()),
            truncated: false,
            redacted: false,
            captured_at: None,
        }
    }

    #[cfg(unix)]
    fn wait_for_child_pid(path: &std::path::Path) -> i32 {
        for _ in 0..20 {
            if let Ok(value) = fs::read_to_string(path) {
                if let Ok(pid) = value.trim().parse::<i32>() {
                    return pid;
                }
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!("child pid file was not written")
    }

    #[cfg(unix)]
    fn unix_process_exists(pid: i32) -> bool {
        let result = unsafe { libc::kill(pid, 0) };
        if result == 0 {
            return true;
        }
        std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }
}
