use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use regex::Regex;
use serde_json::Value as JsonValue;

use super::{
    policy::system_diagnostics_policy_trace, AutonomousSystemDiagnosticsAction,
    AutonomousSystemDiagnosticsArtifact, AutonomousSystemDiagnosticsArtifactMode,
    AutonomousSystemDiagnosticsDiagnostic, AutonomousSystemDiagnosticsFdKind,
    AutonomousSystemDiagnosticsLogLevel, AutonomousSystemDiagnosticsOutput,
    AutonomousSystemDiagnosticsPolicyTrace, AutonomousSystemDiagnosticsPreset,
    AutonomousSystemDiagnosticsRequest, AutonomousSystemDiagnosticsRow,
    AutonomousSystemDiagnosticsTarget, AutonomousToolOutput, AutonomousToolResult,
    AutonomousToolRuntime, AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS,
};
use crate::{
    commands::{validate_non_empty, CommandError, CommandResult},
    runtime::{cancelled_error, redaction::find_prohibited_persistence_content},
};

const DEFAULT_SYSTEM_DIAGNOSTICS_LIMIT: usize = 100;
const MAX_SYSTEM_DIAGNOSTICS_LIMIT: usize = 500;
const MAX_SYSTEM_DIAGNOSTICS_FILTER_CHARS: usize = 256;
const MAX_SYSTEM_DIAGNOSTICS_DURATION_MS: u64 = 10_000;
const MAX_SYSTEM_DIAGNOSTICS_ARTIFACT_BYTES: usize = 8 * 1024 * 1024;
const MAX_SYSTEM_DIAGNOSTICS_DEPTH: usize = 8;
const DEFAULT_PROCESS_SAMPLE_DURATION_MS: u64 = 2_000;
const DEFAULT_PROCESS_SAMPLE_INTERVAL_MS: u64 = 10;
const PROCESS_SAMPLE_TIMEOUT_GRACE_MS: u64 = 3_000;
const DEFAULT_SYSTEM_LOG_LAST_MS: u64 = 60_000;
const MAX_SYSTEM_LOG_LAST_MS: u64 = 15 * 60_000;
const MAX_SYSTEM_LOG_MESSAGE_CHARS: usize = 320;
const SYSTEM_DIAGNOSTICS_ARTIFACT_DIR: &str = "diagnostics-artifacts";

impl AutonomousToolRuntime {
    pub fn system_diagnostics(
        &self,
        request: AutonomousSystemDiagnosticsRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.system_diagnostics_with_approval(request, false)
    }

    pub fn system_diagnostics_with_operator_approval(
        &self,
        request: AutonomousSystemDiagnosticsRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.system_diagnostics_with_approval(request, true)
    }

    fn system_diagnostics_with_approval(
        &self,
        request: AutonomousSystemDiagnosticsRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        validate_system_diagnostics_request(&request)?;
        let target = diagnostics_target_from_request(&request);
        let policy = system_diagnostics_policy_trace(request.action);
        let platform_supported = platform_supported_for_action(request.action);

        if !platform_supported {
            return Ok(system_diagnostics_result(SystemDiagnosticsResultInput {
                action: request.action,
                platform_supported,
                performed: false,
                target,
                policy,
                summary: format!(
                    "System diagnostics action `{}` is not supported on this platform yet.",
                    system_diagnostics_action_label(request.action)
                ),
                rows: Vec::new(),
                truncated: false,
                redacted: false,
                artifact: None,
                diagnostics: vec![diagnostic(
                    "system_diagnostics_platform_unsupported",
                    "This diagnostics action is not supported by the current desktop platform.",
                )],
            }));
        }

        if policy.approval_required && !operator_approved {
            return Ok(system_diagnostics_result(SystemDiagnosticsResultInput {
                action: request.action,
                platform_supported,
                performed: false,
                target,
                policy: policy.clone(),
                summary: format!(
                    "System diagnostics action `{}` requires operator review before Xero can run it.",
                    system_diagnostics_action_label(request.action)
                ),
                rows: Vec::new(),
                truncated: false,
                redacted: false,
                artifact: None,
                diagnostics: vec![diagnostic(
                    "system_diagnostics_approval_required",
                    "Operator approval would allow this diagnostics action to run with the exact same input.",
                )],
            }));
        }

        match request.action {
            AutonomousSystemDiagnosticsAction::ProcessOpenFiles => {
                self.process_open_files(request, target, policy)
            }
            AutonomousSystemDiagnosticsAction::ProcessResourceSnapshot => {
                self.process_resource_snapshot(request, target, policy)
            }
            AutonomousSystemDiagnosticsAction::ProcessThreads => {
                self.process_threads(request, target, policy)
            }
            AutonomousSystemDiagnosticsAction::ProcessSample => {
                self.process_sample(request, target, policy)
            }
            AutonomousSystemDiagnosticsAction::SystemLogQuery => {
                self.system_log_query(request, target, policy)
            }
            AutonomousSystemDiagnosticsAction::MacosAccessibilitySnapshot => {
                self.macos_accessibility_snapshot(request, target, policy)
            }
            AutonomousSystemDiagnosticsAction::DiagnosticsBundle => {
                self.diagnostics_bundle(request, target, policy, operator_approved)
            }
        }
    }

    fn process_open_files(
        &self,
        request: AutonomousSystemDiagnosticsRequest,
        mut target: AutonomousSystemDiagnosticsTarget,
        policy: AutonomousSystemDiagnosticsPolicyTrace,
    ) -> CommandResult<AutonomousToolResult> {
        let limit = bounded_limit(request.limit)?;
        let pid = request.pid.ok_or_else(|| {
            CommandError::user_fixable(
                "system_diagnostics_pid_required",
                "Xero requires process_open_files diagnostics to include pid.",
            )
        })?;
        let filter = request
            .filter
            .as_deref()
            .map(Regex::new)
            .transpose()
            .map_err(|error| {
                CommandError::user_fixable(
                    "system_diagnostics_filter_invalid",
                    format!("Xero could not compile system diagnostics filter regex: {error}"),
                )
            })?;

        let mut result = platform_process_open_files(pid)?;
        result
            .rows
            .retain(|row| row_matches_request(row, &request, filter.as_ref()));
        let total = result.rows.len();
        result.rows.sort_by(|left, right| {
            left.fd
                .cmp(&right.fd)
                .then_with(|| left.fd_kind.cmp(&right.fd_kind))
                .then_with(|| left.path.cmp(&right.path))
                .then_with(|| left.local_port.cmp(&right.local_port))
        });
        result.rows.truncate(limit);
        let truncated = total > result.rows.len();
        if target.process_name.is_none() {
            target.process_name = result
                .rows
                .iter()
                .find_map(|row| row.process_name.clone())
                .or(result.process_name);
        }

        let summary = if truncated {
            format!(
                "Inspected {} open file/socket row(s) for PID {pid}, truncated from {total}.",
                result.rows.len()
            )
        } else {
            format!(
                "Inspected {} open file/socket row(s) for PID {pid}.",
                result.rows.len()
            )
        };

        Ok(system_diagnostics_result(SystemDiagnosticsResultInput {
            action: AutonomousSystemDiagnosticsAction::ProcessOpenFiles,
            platform_supported: true,
            performed: true,
            target,
            policy,
            summary,
            rows: result.rows,
            truncated,
            redacted: result.redacted,
            artifact: None,
            diagnostics: result.diagnostics,
        }))
    }

    fn process_resource_snapshot(
        &self,
        request: AutonomousSystemDiagnosticsRequest,
        mut target: AutonomousSystemDiagnosticsTarget,
        policy: AutonomousSystemDiagnosticsPolicyTrace,
    ) -> CommandResult<AutonomousToolResult> {
        let pid = required_pid(
            &request,
            "process_resource_snapshot",
            "Xero requires process_resource_snapshot diagnostics to include pid.",
        )?;
        let mut result = platform_process_resource_snapshot(pid, &request)?;
        if let Some(row) = result.rows.first_mut() {
            if request.include_threads_summary && row.thread_count.is_none() {
                match platform_process_threads(pid, &request) {
                    Ok(thread_result) => {
                        row.thread_count = Some(thread_result.rows.len() as u32);
                    }
                    Err(error) => result.diagnostics.push(diagnostic(
                        "system_diagnostics_thread_summary_unavailable",
                        format!(
                            "Xero could not include thread summary data: {}",
                            error.message
                        ),
                    )),
                }
            }
            if request.include_ports {
                match platform_process_open_files(pid) {
                    Ok(open_files) => apply_port_summary(row, open_files.rows),
                    Err(error) => result.diagnostics.push(diagnostic(
                        "system_diagnostics_port_summary_unavailable",
                        format!(
                            "Xero could not include port summary data: {}",
                            error.message
                        ),
                    )),
                }
            }
        }
        if request.include_children {
            result.diagnostics.push(diagnostic(
                "system_diagnostics_include_children_not_applied",
                "process_resource_snapshot currently captures the target PID only.",
            ));
        }
        if target.process_name.is_none() {
            target.process_name = result
                .process_name
                .clone()
                .or_else(|| result.rows.iter().find_map(|row| row.process_name.clone()));
        }

        let summary = format!("Captured process resource snapshot for PID {pid}.");
        Ok(system_diagnostics_result(SystemDiagnosticsResultInput {
            action: AutonomousSystemDiagnosticsAction::ProcessResourceSnapshot,
            platform_supported: true,
            performed: true,
            target,
            policy,
            summary,
            rows: result.rows,
            truncated: false,
            redacted: result.redacted,
            artifact: None,
            diagnostics: result.diagnostics,
        }))
    }

    fn process_threads(
        &self,
        request: AutonomousSystemDiagnosticsRequest,
        mut target: AutonomousSystemDiagnosticsTarget,
        policy: AutonomousSystemDiagnosticsPolicyTrace,
    ) -> CommandResult<AutonomousToolResult> {
        let limit = bounded_limit(request.limit)?;
        let pid = required_pid(
            &request,
            "process_threads",
            "Xero requires process_threads diagnostics to include pid.",
        )?;
        let filter = request
            .filter
            .as_deref()
            .map(Regex::new)
            .transpose()
            .map_err(|error| {
                CommandError::user_fixable(
                    "system_diagnostics_filter_invalid",
                    format!("Xero could not compile system diagnostics filter regex: {error}"),
                )
            })?;

        let mut result = platform_process_threads(pid, &request)?;
        result.rows.retain(|row| {
            filter
                .as_ref()
                .is_none_or(|filter| filter.is_match(&searchable_row_text(row)))
        });
        let total = result.rows.len();
        result.rows.sort_by(|left, right| {
            left.thread_id.cmp(&right.thread_id).then_with(|| {
                left.platform
                    .get("thread_index")
                    .cmp(&right.platform.get("thread_index"))
            })
        });
        result.rows.truncate(limit);
        let truncated = total > result.rows.len();
        if target.process_name.is_none() {
            target.process_name = result
                .process_name
                .clone()
                .or_else(|| result.rows.iter().find_map(|row| row.process_name.clone()));
        }

        let summary = if truncated {
            format!(
                "Inspected {} thread row(s) for PID {pid}, truncated from {total}.",
                result.rows.len()
            )
        } else {
            format!(
                "Inspected {} thread row(s) for PID {pid}.",
                result.rows.len()
            )
        };
        Ok(system_diagnostics_result(SystemDiagnosticsResultInput {
            action: AutonomousSystemDiagnosticsAction::ProcessThreads,
            platform_supported: true,
            performed: true,
            target,
            policy,
            summary,
            rows: result.rows,
            truncated,
            redacted: result.redacted,
            artifact: None,
            diagnostics: result.diagnostics,
        }))
    }

    fn process_sample(
        &self,
        request: AutonomousSystemDiagnosticsRequest,
        mut target: AutonomousSystemDiagnosticsTarget,
        policy: AutonomousSystemDiagnosticsPolicyTrace,
    ) -> CommandResult<AutonomousToolResult> {
        let pid = required_pid(
            &request,
            "process_sample",
            "Xero requires process_sample diagnostics to include pid.",
        )?;
        let duration_ms = request
            .duration_ms
            .unwrap_or(DEFAULT_PROCESS_SAMPLE_DURATION_MS);
        let interval_ms = request
            .interval_ms
            .unwrap_or(DEFAULT_PROCESS_SAMPLE_INTERVAL_MS);
        let max_artifact_bytes = request
            .max_artifact_bytes
            .unwrap_or(MAX_SYSTEM_DIAGNOSTICS_ARTIFACT_BYTES);
        if target.process_name.is_none() {
            if let Ok(snapshot) = platform_process_resource_snapshot(pid, &request) {
                target.process_name = snapshot.process_name;
            }
        }

        self.check_cancelled()?;
        let artifact_path = self.next_system_diagnostics_artifact_path("process-sample", "txt")?;
        let sample = platform_process_sample(self, pid, duration_ms, interval_ms, &artifact_path)?;
        self.check_cancelled()?;

        let (raw_text, raw_truncated) =
            read_text_file_prefix(&artifact_path, max_artifact_bytes.saturating_add(1))
                .unwrap_or_else(|_| (String::new(), false));
        let source_text = if raw_text.trim().is_empty() && !sample.stderr.trim().is_empty() {
            sample.stderr.clone()
        } else {
            raw_text
        };
        let source_byte_count = fs::metadata(&artifact_path)
            .map(|metadata| metadata.len() as usize)
            .unwrap_or_else(|_| source_text.len());
        let (redacted_text, redacted) = redact_text_for_diagnostics(&source_text, usize::MAX);
        let (artifact_text, artifact_truncated) =
            truncate_text_to_bytes(&redacted_text, max_artifact_bytes);
        write_text_artifact(&artifact_path, &artifact_text)?;

        let mut diagnostics = sample.diagnostics;
        if raw_truncated || artifact_truncated {
            diagnostics.push(diagnostic(
                "system_diagnostics_process_sample_artifact_truncated",
                format!(
                    "Xero truncated the process sample artifact to {max_artifact_bytes} byte(s)."
                ),
            ));
        }
        if sample.timed_out {
            diagnostics.push(diagnostic(
                "system_diagnostics_process_sample_timeout",
                "Xero stopped the process sampler after the bounded timeout and reaped the sampler process.",
            ));
        }
        if !sample.status_success {
            diagnostics.push(diagnostic(
                "system_diagnostics_process_sample_failed",
                sample.stderr_excerpt.unwrap_or_else(|| {
                    "The process sampler exited unsuccessfully without diagnostic stderr.".into()
                }),
            ));
        }

        let performed = sample.status_success && !sample.timed_out;
        let mut row = empty_row("process_sample", Some(pid), target.process_name.clone());
        row.message = sample_profile_excerpt(&artifact_text);
        row.platform.insert("source".into(), "sample".into());
        row.platform
            .insert("duration_ms".into(), duration_ms.to_string());
        row.platform
            .insert("interval_ms".into(), interval_ms.to_string());
        row.platform
            .insert("wall_ms".into(), sample.wall_ms.to_string());
        row.platform
            .insert("exit_status".into(), sample.exit_status.clone());
        row.platform.insert(
            "artifact_path".into(),
            artifact_path.to_string_lossy().into_owned(),
        );
        row.platform
            .insert("artifact_bytes".into(), artifact_text.len().to_string());
        row.platform
            .insert("source_bytes".into(), source_byte_count.to_string());
        if sample.timed_out {
            row.platform.insert("timed_out".into(), "true".into());
        }

        let summary = if performed {
            format!(
                "Sampled PID {pid} for {duration_ms} ms at {interval_ms} ms intervals and wrote a bounded artifact."
            )
        } else if sample.timed_out {
            format!(
                "Process sampling for PID {pid} timed out after {} ms; any partial output was bounded in the artifact.",
                sample.wall_ms
            )
        } else {
            format!(
                "Process sampling for PID {pid} exited unsuccessfully; stderr and any partial output were bounded in the artifact."
            )
        };

        Ok(system_diagnostics_result(SystemDiagnosticsResultInput {
            action: AutonomousSystemDiagnosticsAction::ProcessSample,
            platform_supported: true,
            performed,
            target,
            policy,
            summary,
            rows: vec![row],
            truncated: raw_truncated || artifact_truncated || sample.timed_out,
            redacted,
            artifact: Some(AutonomousSystemDiagnosticsArtifact {
                path: artifact_path.to_string_lossy().into_owned(),
                byte_count: artifact_text.len(),
                redacted,
                truncated: raw_truncated || artifact_truncated,
            }),
            diagnostics,
        }))
    }

    fn system_log_query(
        &self,
        request: AutonomousSystemDiagnosticsRequest,
        target: AutonomousSystemDiagnosticsTarget,
        policy: AutonomousSystemDiagnosticsPolicyTrace,
    ) -> CommandResult<AutonomousToolResult> {
        let limit = bounded_limit(request.limit)?;
        let log_window = bounded_log_last_ms(request.last_ms);
        let mut result = platform_system_log_query(&request, limit, log_window)?;
        let artifact =
            if request.artifact_mode == Some(AutonomousSystemDiagnosticsArtifactMode::Full) {
                let (artifact, artifact_truncated) = self.write_system_diagnostics_artifact(
                    "system-log-query",
                    &result.artifact_text,
                    result.redacted,
                    request
                        .max_artifact_bytes
                        .unwrap_or(MAX_SYSTEM_DIAGNOSTICS_ARTIFACT_BYTES),
                )?;
                result.truncated |= artifact_truncated;
                Some(artifact)
            } else {
                None
            };
        if request
            .last_ms
            .is_some_and(|value| value > MAX_SYSTEM_LOG_LAST_MS)
        {
            result.diagnostics.push(diagnostic(
                "system_diagnostics_log_window_clamped",
                format!("Xero clamped system_log_query lastMs to {MAX_SYSTEM_LOG_LAST_MS} ms."),
            ));
        }
        if request
            .limit
            .is_some_and(|value| value > MAX_SYSTEM_DIAGNOSTICS_LIMIT)
        {
            result.diagnostics.push(diagnostic(
                "system_diagnostics_limit_clamped",
                format!(
                    "Xero clamped system_log_query limit to {MAX_SYSTEM_DIAGNOSTICS_LIMIT} row(s)."
                ),
            ));
        }

        let summary = if result.truncated {
            format!(
                "Queried recent macOS logs and returned {} row(s), truncated to configured bounds.",
                result.rows.len()
            )
        } else {
            format!(
                "Queried recent macOS logs and returned {} row(s).",
                result.rows.len()
            )
        };
        Ok(system_diagnostics_result(SystemDiagnosticsResultInput {
            action: AutonomousSystemDiagnosticsAction::SystemLogQuery,
            platform_supported: true,
            performed: true,
            target,
            policy,
            summary,
            rows: result.rows,
            truncated: result.truncated,
            redacted: result.redacted,
            artifact,
            diagnostics: result.diagnostics,
        }))
    }

    fn macos_accessibility_snapshot(
        &self,
        request: AutonomousSystemDiagnosticsRequest,
        mut target: AutonomousSystemDiagnosticsTarget,
        policy: AutonomousSystemDiagnosticsPolicyTrace,
    ) -> CommandResult<AutonomousToolResult> {
        let limit = bounded_limit(request.limit)?;
        let mut result = platform_macos_accessibility_snapshot(&request, limit)?;
        if target.pid.is_none() {
            target.pid = result.target.pid;
        }
        if target.process_name.is_none() {
            target.process_name = result.target.process_name.take();
        }
        if target.bundle_id.is_none() {
            target.bundle_id = result.target.bundle_id.take();
        }
        if target.app_name.is_none() {
            target.app_name = result.target.app_name.take();
        }
        if target.window_id.is_none() {
            target.window_id = result.target.window_id;
        }

        let summary = if result.performed {
            if result.truncated {
                format!(
                    "Captured {} macOS Accessibility row(s), truncated to configured bounds.",
                    result.rows.len()
                )
            } else {
                format!("Captured {} macOS Accessibility row(s).", result.rows.len())
            }
        } else {
            "macOS Accessibility snapshot could not run; see diagnostics for the required user action."
                .into()
        };

        Ok(system_diagnostics_result(SystemDiagnosticsResultInput {
            action: AutonomousSystemDiagnosticsAction::MacosAccessibilitySnapshot,
            platform_supported: true,
            performed: result.performed,
            target,
            policy,
            summary,
            rows: result.rows,
            truncated: result.truncated,
            redacted: result.redacted,
            artifact: None,
            diagnostics: result.diagnostics,
        }))
    }

    fn diagnostics_bundle(
        &self,
        request: AutonomousSystemDiagnosticsRequest,
        target: AutonomousSystemDiagnosticsTarget,
        mut policy: AutonomousSystemDiagnosticsPolicyTrace,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        let preset = request.preset.ok_or_else(|| {
            CommandError::user_fixable(
                "system_diagnostics_bundle_preset_required",
                "Xero requires diagnostics_bundle requests to include a preset.",
            )
        })?;
        if !diagnostics_bundle_platform_supported(preset) {
            return Ok(system_diagnostics_result(SystemDiagnosticsResultInput {
                action: AutonomousSystemDiagnosticsAction::DiagnosticsBundle,
                platform_supported: false,
                performed: false,
                target,
                policy,
                summary: format!(
                    "Diagnostics bundle preset `{}` is not supported on this platform.",
                    diagnostics_bundle_preset_label(preset)
                ),
                rows: Vec::new(),
                truncated: false,
                redacted: false,
                artifact: None,
                diagnostics: vec![diagnostic(
                    "system_diagnostics_bundle_platform_unsupported",
                    "This diagnostics bundle preset is not supported by the current desktop platform.",
                )],
            }));
        }

        let limit = bounded_limit(request.limit)?;
        let mut bundle = DiagnosticsBundleAccumulator::new(preset, target, limit);
        for step in diagnostics_bundle_plan(preset, &request) {
            self.check_cancelled()?;
            match self.system_diagnostics_with_approval(step, operator_approved) {
                Ok(result) => {
                    let AutonomousToolOutput::SystemDiagnostics(output) = result.output else {
                        bundle.record_error(
                            AutonomousSystemDiagnosticsAction::DiagnosticsBundle,
                            "Xero received an unexpected output while composing diagnostics bundle.",
                        );
                        continue;
                    };
                    let blocked = bundle.merge_output(output);
                    if blocked && !operator_approved {
                        break;
                    }
                }
                Err(error) => {
                    bundle.record_error(error_action_from_code(&error.code), error.message)
                }
            }
        }

        if bundle.blocked_approval_count > 0 && !operator_approved {
            policy.approval_required = true;
            policy.code = "system_diagnostics_bundle_requires_approval".into();
            policy.reason = format!(
                "Diagnostics bundle `{}` reached {} approval-gated action(s). Approving reruns the same typed bundle without bypassing individual action policy.",
                diagnostics_bundle_preset_label(preset),
                bundle.blocked_approval_count
            );
            if bundle.blocked_os_automation {
                policy.risk_level = super::AutonomousProcessActionRiskLevel::OsAutomation;
            }
        }

        let approval_blocked = policy.approval_required && !operator_approved;
        let summary = bundle.summary(approval_blocked);
        Ok(system_diagnostics_result(SystemDiagnosticsResultInput {
            action: AutonomousSystemDiagnosticsAction::DiagnosticsBundle,
            platform_supported: true,
            performed: bundle.performed_count > 0 && !approval_blocked,
            target: bundle.target,
            policy,
            summary,
            rows: bundle.rows,
            truncated: bundle.truncated,
            redacted: bundle.redacted,
            artifact: bundle.artifact,
            diagnostics: bundle.diagnostics,
        }))
    }

    fn write_system_diagnostics_artifact(
        &self,
        prefix: &str,
        text: &str,
        redacted: bool,
        max_bytes: usize,
    ) -> CommandResult<(AutonomousSystemDiagnosticsArtifact, bool)> {
        let root = crate::db::project_app_data_dir_for_repo(&self.repo_root)
            .join(SYSTEM_DIAGNOSTICS_ARTIFACT_DIR);
        fs::create_dir_all(&root).map_err(|error| {
            CommandError::system_fault(
                "system_diagnostics_artifact_failed",
                format!(
                    "Xero could not create diagnostics artifact directory {}: {error}",
                    root.display()
                ),
            )
        })?;
        let path = diagnostics_artifact_path(&root, prefix, "json")?;
        let (content, truncated) = truncate_text_to_bytes(text, max_bytes);
        fs::write(&path, content.as_bytes()).map_err(|error| {
            CommandError::system_fault(
                "system_diagnostics_artifact_failed",
                format!(
                    "Xero could not write diagnostics artifact {}: {error}",
                    path.display()
                ),
            )
        })?;
        Ok((
            AutonomousSystemDiagnosticsArtifact {
                path: path.display().to_string(),
                byte_count: content.len(),
                redacted,
                truncated,
            },
            truncated,
        ))
    }

    fn next_system_diagnostics_artifact_path(
        &self,
        prefix: &str,
        extension: &str,
    ) -> CommandResult<PathBuf> {
        let root = crate::db::project_app_data_dir_for_repo(&self.repo_root)
            .join(SYSTEM_DIAGNOSTICS_ARTIFACT_DIR);
        fs::create_dir_all(&root).map_err(|error| {
            CommandError::system_fault(
                "system_diagnostics_artifact_failed",
                format!(
                    "Xero could not create diagnostics artifact directory {}: {error}",
                    root.display()
                ),
            )
        })?;
        diagnostics_artifact_path(&root, prefix, extension)
    }
}

#[derive(Debug)]
struct ProcessOpenFilesResult {
    rows: Vec<AutonomousSystemDiagnosticsRow>,
    process_name: Option<String>,
    redacted: bool,
    diagnostics: Vec<AutonomousSystemDiagnosticsDiagnostic>,
}

#[derive(Debug)]
struct DiagnosticsRowsResult {
    rows: Vec<AutonomousSystemDiagnosticsRow>,
    process_name: Option<String>,
    redacted: bool,
    diagnostics: Vec<AutonomousSystemDiagnosticsDiagnostic>,
}

#[derive(Debug)]
struct SystemLogQueryResult {
    rows: Vec<AutonomousSystemDiagnosticsRow>,
    artifact_text: String,
    redacted: bool,
    truncated: bool,
    diagnostics: Vec<AutonomousSystemDiagnosticsDiagnostic>,
}

#[derive(Debug)]
struct ProcessSampleRunResult {
    status_success: bool,
    timed_out: bool,
    exit_status: String,
    wall_ms: u64,
    stderr: String,
    stderr_excerpt: Option<String>,
    diagnostics: Vec<AutonomousSystemDiagnosticsDiagnostic>,
}

#[derive(Debug)]
struct MacosAccessibilitySnapshotResult {
    performed: bool,
    target: AutonomousSystemDiagnosticsTarget,
    rows: Vec<AutonomousSystemDiagnosticsRow>,
    truncated: bool,
    redacted: bool,
    diagnostics: Vec<AutonomousSystemDiagnosticsDiagnostic>,
}

struct SystemDiagnosticsResultInput {
    action: AutonomousSystemDiagnosticsAction,
    platform_supported: bool,
    performed: bool,
    target: AutonomousSystemDiagnosticsTarget,
    policy: AutonomousSystemDiagnosticsPolicyTrace,
    summary: String,
    rows: Vec<AutonomousSystemDiagnosticsRow>,
    truncated: bool,
    redacted: bool,
    artifact: Option<AutonomousSystemDiagnosticsArtifact>,
    diagnostics: Vec<AutonomousSystemDiagnosticsDiagnostic>,
}

struct DiagnosticsBundleAccumulator {
    preset: AutonomousSystemDiagnosticsPreset,
    target: AutonomousSystemDiagnosticsTarget,
    limit: usize,
    rows: Vec<AutonomousSystemDiagnosticsRow>,
    truncated: bool,
    redacted: bool,
    artifact: Option<AutonomousSystemDiagnosticsArtifact>,
    diagnostics: Vec<AutonomousSystemDiagnosticsDiagnostic>,
    performed_count: usize,
    failed_count: usize,
    unsupported_count: usize,
    blocked_approval_count: usize,
    blocked_os_automation: bool,
}

impl DiagnosticsBundleAccumulator {
    fn new(
        preset: AutonomousSystemDiagnosticsPreset,
        target: AutonomousSystemDiagnosticsTarget,
        limit: usize,
    ) -> Self {
        Self {
            preset,
            target,
            limit,
            rows: Vec::new(),
            truncated: false,
            redacted: false,
            artifact: None,
            diagnostics: Vec::new(),
            performed_count: 0,
            failed_count: 0,
            unsupported_count: 0,
            blocked_approval_count: 0,
            blocked_os_automation: false,
        }
    }

    fn merge_output(&mut self, output: AutonomousSystemDiagnosticsOutput) -> bool {
        let action = output.action;
        let blocked = diagnostics_output_blocked_by_approval(&output);
        self.merge_target(&output.target);
        self.truncated |= output.truncated;
        self.redacted |= output.redacted;
        if output.performed {
            self.performed_count += 1;
        } else if blocked {
            self.blocked_approval_count += 1;
            self.blocked_os_automation |=
                action == AutonomousSystemDiagnosticsAction::MacosAccessibilitySnapshot;
            self.diagnostics.push(diagnostic(
                "system_diagnostics_bundle_action_blocked",
                format!(
                    "Diagnostics bundle `{}` stopped before `{}` because that typed action requires operator approval.",
                    diagnostics_bundle_preset_label(self.preset),
                    system_diagnostics_action_label(action)
                ),
            ));
        } else if !output.platform_supported {
            self.unsupported_count += 1;
        } else {
            self.failed_count += 1;
        }

        self.push_row(diagnostics_bundle_section_row(self.preset, &output));
        let artifact = output.artifact.clone();
        for mut row in output.rows {
            annotate_bundle_row(self.preset, action, &mut row);
            self.push_row(row);
        }
        for diagnostic in output.diagnostics {
            self.diagnostics
                .push(annotate_bundle_diagnostic(action, diagnostic));
        }
        if let Some(artifact) = artifact {
            if self.artifact.is_none() {
                self.artifact = Some(artifact);
            } else {
                self.diagnostics.push(diagnostic(
                    "system_diagnostics_bundle_artifact_omitted",
                    "Diagnostics bundle produced more than one artifact; Xero returned the first artifact and kept the report compact.",
                ));
            }
        }

        blocked
    }

    fn record_error(
        &mut self,
        action: AutonomousSystemDiagnosticsAction,
        message: impl Into<String>,
    ) {
        let message = message.into();
        self.failed_count += 1;
        let mut row = empty_row(
            "diagnostics_bundle_section",
            self.target.pid,
            self.target.process_name.clone(),
        );
        row.state = Some("failed".into());
        row.message = Some(message.clone());
        row.platform.insert(
            "bundle_preset".into(),
            diagnostics_bundle_preset_label(self.preset).into(),
        );
        row.platform.insert(
            "bundle_action".into(),
            system_diagnostics_action_label(action).into(),
        );
        row.platform.insert("performed".into(), "false".into());
        self.push_row(row);
        self.diagnostics.push(diagnostic(
            "system_diagnostics_bundle_action_failed",
            format!(
                "Diagnostics bundle `{}` could not run `{}`: {message}",
                diagnostics_bundle_preset_label(self.preset),
                system_diagnostics_action_label(action)
            ),
        ));
    }

    fn summary(&self, approval_blocked: bool) -> String {
        let preset = diagnostics_bundle_preset_label(self.preset);
        if approval_blocked {
            return format!(
                "Diagnostics bundle `{preset}` captured {} safe section(s) and stopped at {} approval boundary(s).",
                self.performed_count, self.blocked_approval_count
            );
        }

        let mut details = Vec::new();
        if self.failed_count > 0 {
            details.push(format!("{} failed", self.failed_count));
        }
        if self.unsupported_count > 0 {
            details.push(format!("{} unsupported", self.unsupported_count));
        }
        if self.truncated {
            details.push("truncated".into());
        }
        if details.is_empty() {
            format!(
                "Diagnostics bundle `{preset}` captured {} section(s) into {} compact row(s).",
                self.performed_count,
                self.rows.len()
            )
        } else {
            format!(
                "Diagnostics bundle `{preset}` captured {} section(s) into {} compact row(s); {}.",
                self.performed_count,
                self.rows.len(),
                details.join(", ")
            )
        }
    }

    fn push_row(&mut self, row: AutonomousSystemDiagnosticsRow) {
        if self.rows.len() >= self.limit {
            self.truncated = true;
            return;
        }
        self.rows.push(row);
    }

    fn merge_target(&mut self, target: &AutonomousSystemDiagnosticsTarget) {
        if self.target.pid.is_none() {
            self.target.pid = target.pid;
        }
        if self.target.process_name.is_none() {
            self.target.process_name = target.process_name.clone();
        }
        if self.target.bundle_id.is_none() {
            self.target.bundle_id = target.bundle_id.clone();
        }
        if self.target.app_name.is_none() {
            self.target.app_name = target.app_name.clone();
        }
        if self.target.window_id.is_none() {
            self.target.window_id = target.window_id;
        }
    }
}

fn system_diagnostics_result(input: SystemDiagnosticsResultInput) -> AutonomousToolResult {
    AutonomousToolResult {
        tool_name: AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS.into(),
        summary: input.summary.clone(),
        command_result: None,
        output: AutonomousToolOutput::SystemDiagnostics(AutonomousSystemDiagnosticsOutput {
            action: input.action,
            platform_supported: input.platform_supported,
            performed: input.performed,
            target: input.target,
            policy: input.policy,
            summary: input.summary,
            rows: input.rows,
            truncated: input.truncated,
            redacted: input.redacted,
            artifact: input.artifact,
            diagnostics: input.diagnostics,
        }),
    }
}

fn diagnostics_bundle_plan(
    preset: AutonomousSystemDiagnosticsPreset,
    request: &AutonomousSystemDiagnosticsRequest,
) -> Vec<AutonomousSystemDiagnosticsRequest> {
    let mut steps = Vec::new();
    match preset {
        AutonomousSystemDiagnosticsPreset::HungProcess => {
            push_pid_resource_step(&mut steps, request, true, true);
            push_pid_open_files_step(&mut steps, request, BundleOpenFilesMode::PortsAndIdentity);
            push_pid_threads_step(&mut steps, request);
            push_log_step(&mut steps, request);
            push_pid_sample_step(&mut steps, request);
        }
        AutonomousSystemDiagnosticsPreset::PortConflict => {
            push_pid_resource_step(&mut steps, request, true, false);
            push_pid_open_files_step(&mut steps, request, BundleOpenFilesMode::SocketsOnly);
            push_log_step(&mut steps, request);
        }
        AutonomousSystemDiagnosticsPreset::TauriWindowIssue => {
            push_pid_resource_step(&mut steps, request, true, true);
            push_pid_open_files_step(&mut steps, request, BundleOpenFilesMode::PortsAndIdentity);
            push_log_step(&mut steps, request);
            push_accessibility_step(&mut steps, request);
        }
        AutonomousSystemDiagnosticsPreset::MacosAppFocusIssue => {
            push_pid_resource_step(&mut steps, request, false, true);
            push_log_step(&mut steps, request);
            push_accessibility_step(&mut steps, request);
        }
        AutonomousSystemDiagnosticsPreset::HighCpuProcess => {
            push_pid_resource_step(&mut steps, request, true, true);
            push_pid_threads_step(&mut steps, request);
            push_log_step(&mut steps, request);
            push_pid_sample_step(&mut steps, request);
        }
    }
    steps
}

#[derive(Debug, Clone, Copy)]
enum BundleOpenFilesMode {
    SocketsOnly,
    PortsAndIdentity,
}

fn push_pid_resource_step(
    steps: &mut Vec<AutonomousSystemDiagnosticsRequest>,
    request: &AutonomousSystemDiagnosticsRequest,
    include_ports: bool,
    include_threads_summary: bool,
) {
    if request.pid.is_none() {
        return;
    }
    let mut step = bundle_step_request(
        request,
        AutonomousSystemDiagnosticsAction::ProcessResourceSnapshot,
        1,
    );
    step.include_ports = include_ports || request.include_ports;
    step.include_threads_summary = include_threads_summary || request.include_threads_summary;
    steps.push(step);
}

fn push_pid_open_files_step(
    steps: &mut Vec<AutonomousSystemDiagnosticsRequest>,
    request: &AutonomousSystemDiagnosticsRequest,
    mode: BundleOpenFilesMode,
) {
    if request.pid.is_none() {
        return;
    }
    let mut step = bundle_step_request(
        request,
        AutonomousSystemDiagnosticsAction::ProcessOpenFiles,
        bundle_limit(request, 40),
    );
    match mode {
        BundleOpenFilesMode::SocketsOnly => {
            step.include_sockets = true;
            step.include_files = false;
            step.include_deleted = false;
        }
        BundleOpenFilesMode::PortsAndIdentity => {
            if step.fd_kinds.is_empty() {
                step.fd_kinds = vec![
                    AutonomousSystemDiagnosticsFdKind::Cwd,
                    AutonomousSystemDiagnosticsFdKind::Executable,
                    AutonomousSystemDiagnosticsFdKind::Socket,
                    AutonomousSystemDiagnosticsFdKind::Deleted,
                ];
            }
        }
    }
    steps.push(step);
}

fn push_pid_threads_step(
    steps: &mut Vec<AutonomousSystemDiagnosticsRequest>,
    request: &AutonomousSystemDiagnosticsRequest,
) {
    if request.pid.is_none() {
        return;
    }
    let mut step = bundle_step_request(
        request,
        AutonomousSystemDiagnosticsAction::ProcessThreads,
        bundle_limit(request, 40),
    );
    step.include_wait_channel = true;
    steps.push(step);
}

fn push_pid_sample_step(
    steps: &mut Vec<AutonomousSystemDiagnosticsRequest>,
    request: &AutonomousSystemDiagnosticsRequest,
) {
    if request.pid.is_none() {
        return;
    }
    let mut step =
        bundle_step_request(request, AutonomousSystemDiagnosticsAction::ProcessSample, 1);
    step.duration_ms = Some(
        request
            .duration_ms
            .unwrap_or(DEFAULT_PROCESS_SAMPLE_DURATION_MS),
    );
    step.interval_ms = Some(
        request
            .interval_ms
            .unwrap_or(DEFAULT_PROCESS_SAMPLE_INTERVAL_MS),
    );
    steps.push(step);
}

fn push_log_step(
    steps: &mut Vec<AutonomousSystemDiagnosticsRequest>,
    request: &AutonomousSystemDiagnosticsRequest,
) {
    let mut step = bundle_step_request(
        request,
        AutonomousSystemDiagnosticsAction::SystemLogQuery,
        bundle_limit(request, 20),
    );
    step.last_ms = Some(bounded_log_last_ms(request.last_ms));
    if system_log_query_has_filter(&step) {
        steps.push(step);
    }
}

fn push_accessibility_step(
    steps: &mut Vec<AutonomousSystemDiagnosticsRequest>,
    request: &AutonomousSystemDiagnosticsRequest,
) {
    if !accessibility_request_has_target(request) {
        return;
    }
    let step = bundle_step_request(
        request,
        AutonomousSystemDiagnosticsAction::MacosAccessibilitySnapshot,
        bundle_limit(request, 30),
    );
    steps.push(step);
}

fn bundle_step_request(
    request: &AutonomousSystemDiagnosticsRequest,
    action: AutonomousSystemDiagnosticsAction,
    limit: usize,
) -> AutonomousSystemDiagnosticsRequest {
    let mut step = request.clone();
    step.action = action;
    step.preset = None;
    step.limit = Some(limit);
    step.include_ports = false;
    step.include_threads_summary = false;
    step.include_wait_channel = false;
    step.include_stack_hints = false;
    step.include_sockets = false;
    step.include_files = false;
    step.include_deleted = false;
    step
}

fn bundle_limit(request: &AutonomousSystemDiagnosticsRequest, cap: usize) -> usize {
    request
        .limit
        .unwrap_or(DEFAULT_SYSTEM_DIAGNOSTICS_LIMIT)
        .min(cap)
        .max(1)
}

fn diagnostics_bundle_section_row(
    preset: AutonomousSystemDiagnosticsPreset,
    output: &AutonomousSystemDiagnosticsOutput,
) -> AutonomousSystemDiagnosticsRow {
    let blocked = diagnostics_output_blocked_by_approval(output);
    let mut row = empty_row(
        "diagnostics_bundle_section",
        output.target.pid,
        output.target.process_name.clone(),
    );
    row.state = Some(
        if output.performed {
            "performed"
        } else if blocked {
            "approval_required"
        } else if !output.platform_supported {
            "unsupported"
        } else {
            "not_performed"
        }
        .into(),
    );
    row.message = Some(output.summary.clone());
    row.platform.insert(
        "bundle_preset".into(),
        diagnostics_bundle_preset_label(preset).into(),
    );
    row.platform.insert(
        "bundle_action".into(),
        system_diagnostics_action_label(output.action).into(),
    );
    row.platform.insert(
        "platform_supported".into(),
        output.platform_supported.to_string(),
    );
    row.platform
        .insert("performed".into(), output.performed.to_string());
    row.platform.insert(
        "approval_required".into(),
        output.policy.approval_required.to_string(),
    );
    row.platform
        .insert("policy_code".into(), output.policy.code.clone());
    row.platform
        .insert("row_count".into(), output.rows.len().to_string());
    if output.truncated {
        row.platform.insert("truncated".into(), "true".into());
    }
    if output.redacted {
        row.platform.insert("redacted".into(), "true".into());
    }
    row
}

fn annotate_bundle_row(
    preset: AutonomousSystemDiagnosticsPreset,
    action: AutonomousSystemDiagnosticsAction,
    row: &mut AutonomousSystemDiagnosticsRow,
) {
    row.platform
        .entry("bundle_preset".into())
        .or_insert_with(|| diagnostics_bundle_preset_label(preset).into());
    row.platform
        .entry("bundle_action".into())
        .or_insert_with(|| system_diagnostics_action_label(action).into());
}

fn annotate_bundle_diagnostic(
    action: AutonomousSystemDiagnosticsAction,
    diagnostic: AutonomousSystemDiagnosticsDiagnostic,
) -> AutonomousSystemDiagnosticsDiagnostic {
    AutonomousSystemDiagnosticsDiagnostic {
        code: diagnostic.code,
        message: format!(
            "{}: {}",
            system_diagnostics_action_label(action),
            diagnostic.message
        ),
    }
}

fn diagnostics_output_blocked_by_approval(output: &AutonomousSystemDiagnosticsOutput) -> bool {
    !output.performed
        && output.policy.approval_required
        && output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "system_diagnostics_approval_required")
}

fn diagnostics_bundle_platform_supported(preset: AutonomousSystemDiagnosticsPreset) -> bool {
    match preset {
        AutonomousSystemDiagnosticsPreset::HungProcess
        | AutonomousSystemDiagnosticsPreset::PortConflict
        | AutonomousSystemDiagnosticsPreset::HighCpuProcess => cfg!(unix),
        AutonomousSystemDiagnosticsPreset::TauriWindowIssue
        | AutonomousSystemDiagnosticsPreset::MacosAppFocusIssue => cfg!(target_os = "macos"),
    }
}

fn diagnostics_bundle_preset_label(preset: AutonomousSystemDiagnosticsPreset) -> &'static str {
    match preset {
        AutonomousSystemDiagnosticsPreset::HungProcess => "hung_process",
        AutonomousSystemDiagnosticsPreset::PortConflict => "port_conflict",
        AutonomousSystemDiagnosticsPreset::TauriWindowIssue => "tauri_window_issue",
        AutonomousSystemDiagnosticsPreset::MacosAppFocusIssue => "macos_app_focus_issue",
        AutonomousSystemDiagnosticsPreset::HighCpuProcess => "high_cpu_process",
    }
}

fn error_action_from_code(code: &str) -> AutonomousSystemDiagnosticsAction {
    if code.contains("open_files") || code.contains("lsof") || code.contains("proc_fd") {
        AutonomousSystemDiagnosticsAction::ProcessOpenFiles
    } else if code.contains("thread") || code.contains("proc_task") {
        AutonomousSystemDiagnosticsAction::ProcessThreads
    } else if code.contains("sample") {
        AutonomousSystemDiagnosticsAction::ProcessSample
    } else if code.contains("log") {
        AutonomousSystemDiagnosticsAction::SystemLogQuery
    } else if code.contains("accessibility") {
        AutonomousSystemDiagnosticsAction::MacosAccessibilitySnapshot
    } else if code.contains("resource") || code.contains("ps_failed") {
        AutonomousSystemDiagnosticsAction::ProcessResourceSnapshot
    } else {
        AutonomousSystemDiagnosticsAction::DiagnosticsBundle
    }
}

fn validate_system_diagnostics_request(
    request: &AutonomousSystemDiagnosticsRequest,
) -> CommandResult<()> {
    if let Some(pid) = request.pid {
        if pid == 0 {
            return Err(CommandError::user_fixable(
                "system_diagnostics_pid_invalid",
                "Xero requires system diagnostics pid to be greater than zero.",
            ));
        }
    }
    if request.action == AutonomousSystemDiagnosticsAction::DiagnosticsBundle {
        validate_diagnostics_bundle_request(request)?;
    } else if request.preset.is_some() {
        return Err(CommandError::user_fixable(
            "system_diagnostics_preset_invalid",
            "Xero only accepts preset for diagnostics_bundle requests.",
        ));
    }
    if request.action == AutonomousSystemDiagnosticsAction::ProcessOpenFiles
        && request.pid.is_none()
    {
        return Err(CommandError::user_fixable(
            "system_diagnostics_pid_required",
            "Xero requires process_open_files diagnostics to include pid.",
        ));
    }
    if matches!(
        request.action,
        AutonomousSystemDiagnosticsAction::ProcessResourceSnapshot
            | AutonomousSystemDiagnosticsAction::ProcessThreads
            | AutonomousSystemDiagnosticsAction::ProcessSample
    ) && request.pid.is_none()
    {
        return Err(CommandError::user_fixable(
            "system_diagnostics_pid_required",
            format!(
                "Xero requires {} diagnostics to include pid.",
                system_diagnostics_action_label(request.action)
            ),
        ));
    }
    if request.action == AutonomousSystemDiagnosticsAction::MacosAccessibilitySnapshot
        && !accessibility_request_has_target(request)
    {
        return Err(CommandError::user_fixable(
            "system_diagnostics_accessibility_target_required",
            "Xero requires macos_accessibility_snapshot to include pid, processName, bundleId, appName, windowId, or focusedOnly.",
        ));
    }
    if request.action == AutonomousSystemDiagnosticsAction::SystemLogQuery
        && !system_log_query_has_filter(request)
    {
        return Err(CommandError::user_fixable(
            "system_diagnostics_log_filter_required",
            "Xero requires system_log_query to include pid, processName, appName, subsystem, category, messageContains, or processPredicate.",
        ));
    }
    let _ = bounded_limit(request.limit)?;
    if let Some(filter) = request.filter.as_deref() {
        validate_non_empty(filter, "filter")?;
        if filter.len() > MAX_SYSTEM_DIAGNOSTICS_FILTER_CHARS {
            return Err(CommandError::user_fixable(
                "system_diagnostics_filter_too_large",
                format!(
                    "Xero limits system diagnostics filter to {MAX_SYSTEM_DIAGNOSTICS_FILTER_CHARS} characters."
                ),
            ));
        }
        Regex::new(filter).map_err(|error| {
            CommandError::user_fixable(
                "system_diagnostics_filter_invalid",
                format!("Xero could not compile system diagnostics filter regex: {error}"),
            )
        })?;
    }
    for (field, value) in [
        ("processName", request.process_name.as_deref()),
        ("bundleId", request.bundle_id.as_deref()),
        ("appName", request.app_name.as_deref()),
        ("since", request.since.as_deref()),
        ("subsystem", request.subsystem.as_deref()),
        ("category", request.category.as_deref()),
        ("messageContains", request.message_contains.as_deref()),
        ("processPredicate", request.process_predicate.as_deref()),
    ] {
        if let Some(value) = value {
            validate_non_empty(value, field)?;
            if value.contains('\0') {
                return Err(CommandError::user_fixable(
                    "system_diagnostics_input_invalid",
                    format!("Xero refused system diagnostics field `{field}` because it contained a NUL byte."),
                ));
            }
        }
    }
    for attribute in &request.attributes {
        validate_non_empty(attribute, "attributes")?;
        if attribute.contains('\0') {
            return Err(CommandError::user_fixable(
                "system_diagnostics_input_invalid",
                "Xero refused system diagnostics attributes containing a NUL byte.",
            ));
        }
    }
    for (field, value) in [
        ("durationMs", request.duration_ms),
        ("intervalMs", request.interval_ms),
    ] {
        if let Some(value) = value {
            if value == 0 || value > MAX_SYSTEM_DIAGNOSTICS_DURATION_MS {
                return Err(CommandError::user_fixable(
                    "system_diagnostics_duration_invalid",
                    format!(
                        "Xero requires {field} to be between 1 and {MAX_SYSTEM_DIAGNOSTICS_DURATION_MS} ms."
                    ),
                ));
            }
        }
    }
    if request.last_ms == Some(0) {
        return Err(CommandError::user_fixable(
            "system_diagnostics_duration_invalid",
            "Xero requires lastMs to be greater than zero.",
        ));
    }
    if let Some(max_artifact_bytes) = request.max_artifact_bytes {
        if max_artifact_bytes == 0 || max_artifact_bytes > MAX_SYSTEM_DIAGNOSTICS_ARTIFACT_BYTES {
            return Err(CommandError::user_fixable(
                "system_diagnostics_artifact_limit_invalid",
                format!(
                    "Xero requires maxArtifactBytes to be between 1 and {MAX_SYSTEM_DIAGNOSTICS_ARTIFACT_BYTES}."
                ),
            ));
        }
    }
    if let Some(max_depth) = request.max_depth {
        if max_depth > MAX_SYSTEM_DIAGNOSTICS_DEPTH {
            return Err(CommandError::user_fixable(
                "system_diagnostics_depth_invalid",
                format!("Xero limits diagnostics maxDepth to {MAX_SYSTEM_DIAGNOSTICS_DEPTH}."),
            ));
        }
    }
    Ok(())
}

fn bounded_limit(limit: Option<usize>) -> CommandResult<usize> {
    let limit = limit.unwrap_or(DEFAULT_SYSTEM_DIAGNOSTICS_LIMIT);
    if limit == 0 {
        return Err(CommandError::user_fixable(
            "system_diagnostics_limit_invalid",
            format!(
                "Xero requires diagnostics limit to be between 1 and {MAX_SYSTEM_DIAGNOSTICS_LIMIT}."
            ),
        ));
    }
    Ok(limit.min(MAX_SYSTEM_DIAGNOSTICS_LIMIT))
}

fn bounded_log_last_ms(last_ms: Option<u64>) -> u64 {
    last_ms
        .unwrap_or(DEFAULT_SYSTEM_LOG_LAST_MS)
        .clamp(1, MAX_SYSTEM_LOG_LAST_MS)
}

fn required_pid(
    request: &AutonomousSystemDiagnosticsRequest,
    action: &str,
    message: &str,
) -> CommandResult<u32> {
    request.pid.ok_or_else(|| {
        CommandError::user_fixable(
            "system_diagnostics_pid_required",
            format!("{message} Action `{action}` needs a numeric pid target."),
        )
    })
}

fn validate_diagnostics_bundle_request(
    request: &AutonomousSystemDiagnosticsRequest,
) -> CommandResult<()> {
    let preset = request.preset.ok_or_else(|| {
        CommandError::user_fixable(
            "system_diagnostics_bundle_preset_required",
            "Xero requires diagnostics_bundle requests to include preset.",
        )
    })?;
    match preset {
        AutonomousSystemDiagnosticsPreset::HungProcess
        | AutonomousSystemDiagnosticsPreset::PortConflict
        | AutonomousSystemDiagnosticsPreset::HighCpuProcess => {
            if request.pid.is_none() {
                return Err(CommandError::user_fixable(
                    "system_diagnostics_pid_required",
                    format!(
                        "Xero requires diagnostics_bundle preset `{}` to include pid.",
                        diagnostics_bundle_preset_label(preset)
                    ),
                ));
            }
        }
        AutonomousSystemDiagnosticsPreset::TauriWindowIssue => {
            if !accessibility_request_has_target(request) {
                return Err(CommandError::user_fixable(
                    "system_diagnostics_bundle_target_required",
                    "Xero requires tauri_window_issue diagnostics bundles to include pid, processName, bundleId, appName, windowId, or focusedOnly.",
                ));
            }
        }
        AutonomousSystemDiagnosticsPreset::MacosAppFocusIssue => {
            if !accessibility_request_has_target(request) {
                return Err(CommandError::user_fixable(
                    "system_diagnostics_bundle_target_required",
                    "Xero requires macos_app_focus_issue diagnostics bundles to include pid, processName, bundleId, appName, windowId, or focusedOnly.",
                ));
            }
        }
    }
    Ok(())
}

fn accessibility_request_has_target(request: &AutonomousSystemDiagnosticsRequest) -> bool {
    request.pid.is_some()
        || request
            .process_name
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || request
            .bundle_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || request
            .app_name
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || request.window_id.is_some()
        || request.focused_only
}

fn system_log_query_has_filter(request: &AutonomousSystemDiagnosticsRequest) -> bool {
    request.pid.is_some()
        || request
            .process_name
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || request
            .app_name
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || request
            .subsystem
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || request
            .category
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || request
            .message_contains
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || request
            .process_predicate
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
}

fn diagnostics_target_from_request(
    request: &AutonomousSystemDiagnosticsRequest,
) -> AutonomousSystemDiagnosticsTarget {
    AutonomousSystemDiagnosticsTarget {
        pid: request.pid,
        process_name: request.process_name.clone(),
        bundle_id: request.bundle_id.clone(),
        app_name: request.app_name.clone(),
        window_id: request.window_id,
    }
}

fn platform_supported_for_action(action: AutonomousSystemDiagnosticsAction) -> bool {
    match action {
        AutonomousSystemDiagnosticsAction::ProcessOpenFiles => cfg!(unix),
        AutonomousSystemDiagnosticsAction::ProcessResourceSnapshot
        | AutonomousSystemDiagnosticsAction::ProcessThreads
        | AutonomousSystemDiagnosticsAction::DiagnosticsBundle => cfg!(unix),
        AutonomousSystemDiagnosticsAction::ProcessSample => cfg!(target_os = "macos"),
        AutonomousSystemDiagnosticsAction::SystemLogQuery
        | AutonomousSystemDiagnosticsAction::MacosAccessibilitySnapshot => {
            cfg!(target_os = "macos")
        }
    }
}

pub(crate) fn system_diagnostics_action_approval_id(
    output: &AutonomousSystemDiagnosticsOutput,
) -> String {
    let target = output
        .target
        .pid
        .map(|pid| format!("pid-{pid}"))
        .or_else(|| output.target.process_name.clone())
        .or_else(|| output.target.bundle_id.clone())
        .or_else(|| output.target.app_name.clone())
        .or_else(|| {
            output
                .target
                .window_id
                .map(|window_id| format!("window-{window_id}"))
        })
        .unwrap_or_else(|| "target".into());
    format!(
        "system-diagnostics-{}-{target}",
        system_diagnostics_action_label(output.action)
    )
}

fn system_diagnostics_action_label(action: AutonomousSystemDiagnosticsAction) -> &'static str {
    match action {
        AutonomousSystemDiagnosticsAction::ProcessOpenFiles => "process_open_files",
        AutonomousSystemDiagnosticsAction::ProcessResourceSnapshot => "process_resource_snapshot",
        AutonomousSystemDiagnosticsAction::ProcessThreads => "process_threads",
        AutonomousSystemDiagnosticsAction::ProcessSample => "process_sample",
        AutonomousSystemDiagnosticsAction::SystemLogQuery => "system_log_query",
        AutonomousSystemDiagnosticsAction::MacosAccessibilitySnapshot => {
            "macos_accessibility_snapshot"
        }
        AutonomousSystemDiagnosticsAction::DiagnosticsBundle => "diagnostics_bundle",
    }
}

fn diagnostic(
    code: impl Into<String>,
    message: impl Into<String>,
) -> AutonomousSystemDiagnosticsDiagnostic {
    AutonomousSystemDiagnosticsDiagnostic {
        code: code.into(),
        message: message.into(),
    }
}

fn row_matches_request(
    row: &AutonomousSystemDiagnosticsRow,
    request: &AutonomousSystemDiagnosticsRequest,
    filter: Option<&Regex>,
) -> bool {
    if !request.fd_kinds.is_empty()
        && row
            .fd_kind
            .is_none_or(|kind| !request.fd_kinds.contains(&kind))
    {
        return false;
    }

    if request.include_sockets || request.include_files || request.include_deleted {
        let selected = if row.deleted {
            request.include_deleted
        } else if row.fd_kind == Some(AutonomousSystemDiagnosticsFdKind::Socket) {
            request.include_sockets
        } else {
            request.include_files
        };
        if !selected {
            return false;
        }
    }

    filter.is_none_or(|filter| filter.is_match(&searchable_row_text(row)))
}

fn searchable_row_text(row: &AutonomousSystemDiagnosticsRow) -> String {
    let mut fields = [
        row.process_name.as_deref(),
        row.fd.as_deref(),
        row.access.as_deref(),
        row.file_type.as_deref(),
        row.protocol.as_deref(),
        row.local_addr.as_deref(),
        row.remote_addr.as_deref(),
        row.state.as_deref(),
        row.path.as_deref(),
        row.cpu_percent.as_deref(),
        row.cpu_time.as_deref(),
        row.priority.as_deref(),
        row.wait_channel.as_deref(),
        row.timestamp.as_deref(),
        row.level.as_deref(),
        row.subsystem.as_deref(),
        row.category.as_deref(),
        row.message.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(ToOwned::to_owned)
    .collect::<Vec<_>>();
    if let Some(port) = row.local_port {
        fields.push(port.to_string());
    }
    if let Some(port) = row.remote_port {
        fields.push(port.to_string());
    }
    if let Some(thread_id) = row.thread_id {
        fields.push(thread_id.to_string());
    }
    if let Some(parent_pid) = row.parent_pid {
        fields.push(parent_pid.to_string());
    }
    if let Some(memory_bytes) = row.memory_bytes {
        fields.push(memory_bytes.to_string());
    }
    if let Some(virtual_memory_bytes) = row.virtual_memory_bytes {
        fields.push(virtual_memory_bytes.to_string());
    }
    if let Some(thread_count) = row.thread_count {
        fields.push(thread_count.to_string());
    }
    if let Some(port_count) = row.port_count {
        fields.push(port_count.to_string());
    }
    fields.extend(row.platform.values().cloned());
    fields.join("\n")
}

fn empty_row(
    row_type: &str,
    pid: Option<u32>,
    process_name: Option<String>,
) -> AutonomousSystemDiagnosticsRow {
    AutonomousSystemDiagnosticsRow {
        row_type: row_type.into(),
        pid,
        parent_pid: None,
        process_name,
        thread_id: None,
        fd: None,
        fd_kind: None,
        access: None,
        file_type: None,
        protocol: None,
        local_addr: None,
        local_port: None,
        remote_addr: None,
        remote_port: None,
        state: None,
        path: None,
        cpu_percent: None,
        cpu_time: None,
        memory_bytes: None,
        virtual_memory_bytes: None,
        thread_count: None,
        port_count: None,
        priority: None,
        wait_channel: None,
        timestamp: None,
        level: None,
        subsystem: None,
        category: None,
        message: None,
        deleted: false,
        platform: BTreeMap::new(),
    }
}

#[cfg(unix)]
fn platform_process_resource_snapshot(
    pid: u32,
    _request: &AutonomousSystemDiagnosticsRequest,
) -> CommandResult<DiagnosticsRowsResult> {
    ps_process_resource_snapshot(pid)
}

#[cfg(not(unix))]
fn platform_process_resource_snapshot(
    _pid: u32,
    _request: &AutonomousSystemDiagnosticsRequest,
) -> CommandResult<DiagnosticsRowsResult> {
    Err(CommandError::user_fixable(
        "system_diagnostics_resource_snapshot_unsupported",
        "Xero process_resource_snapshot diagnostics are not supported on this platform yet.",
    ))
}

#[cfg(unix)]
fn ps_process_resource_snapshot(pid: u32) -> CommandResult<DiagnosticsRowsResult> {
    let output = Command::new("ps")
        .args([
            "-p",
            &pid.to_string(),
            "-o",
            "pid=,ppid=,state=,pcpu=,rss=,vsz=,etime=,time=,comm=",
        ])
        .output()
        .map_err(|error| {
            CommandError::system_fault(
                "system_diagnostics_ps_failed",
                format!("Xero could not execute ps for PID {pid}: {error}"),
            )
        })?;
    if !output.status.success() && output.stdout.is_empty() {
        return Err(CommandError::retryable(
            "system_diagnostics_ps_failed",
            format!("ps exited with status {} for PID {pid}.", output.status),
        ));
    }
    parse_ps_resource_snapshot(pid, String::from_utf8_lossy(&output.stdout).as_ref()).ok_or_else(
        || {
            CommandError::retryable(
                "system_diagnostics_resource_snapshot_empty",
                format!("Xero could not find a process resource snapshot row for PID {pid}."),
            )
        },
    )
}

#[cfg(any(unix, test))]
fn parse_ps_resource_snapshot(pid: u32, text: &str) -> Option<DiagnosticsRowsResult> {
    let mut redacted = false;
    for line in text.lines() {
        let columns = line.split_whitespace().collect::<Vec<_>>();
        if columns.len() < 9
            || columns.first().and_then(|value| value.parse::<u32>().ok()) != Some(pid)
        {
            continue;
        }
        let parent_pid = columns.get(1).and_then(|value| value.parse::<u32>().ok());
        let state = columns.get(2).map(|value| (*value).to_owned());
        let cpu_percent = columns.get(3).map(|value| (*value).to_owned());
        let memory_bytes = columns
            .get(4)
            .and_then(|value| value.parse::<u64>().ok())
            .map(|kb| kb.saturating_mul(1024));
        let virtual_memory_bytes = columns
            .get(5)
            .and_then(|value| value.parse::<u64>().ok())
            .map(|kb| kb.saturating_mul(1024));
        let elapsed = columns.get(6).map(|value| (*value).to_owned());
        let cpu_time = columns.get(7).map(|value| (*value).to_owned());
        let command = columns[8..].join(" ");
        let process_name = process_name_from_command(&command).unwrap_or_else(|| pid.to_string());
        let mut row = empty_row(
            "process_resource_snapshot",
            Some(pid),
            Some(process_name.clone()),
        );
        row.parent_pid = parent_pid.filter(|value| *value != 0);
        row.state = state.map(|value| value.to_ascii_lowercase());
        row.cpu_percent = cpu_percent;
        row.memory_bytes = memory_bytes;
        row.virtual_memory_bytes = virtual_memory_bytes;
        row.cpu_time = cpu_time;
        row.path = process_path_from_command(&command).map(|path| {
            let (path, path_redacted) = redact_path_for_diagnostics(&path);
            redacted |= path_redacted;
            path
        });
        row.platform.insert("source".into(), "ps".into());
        if let Some(elapsed) = elapsed {
            row.platform.insert("elapsed".into(), elapsed);
        }
        if !command.is_empty() {
            let (command, command_redacted) = redact_text_for_diagnostics(&command, 240);
            redacted |= command_redacted;
            row.platform.insert("command".into(), command);
        }
        return Some(DiagnosticsRowsResult {
            rows: vec![row],
            process_name: Some(process_name),
            redacted,
            diagnostics: Vec::new(),
        });
    }
    None
}

fn apply_port_summary(
    row: &mut AutonomousSystemDiagnosticsRow,
    rows: Vec<AutonomousSystemDiagnosticsRow>,
) {
    let mut ports = rows
        .into_iter()
        .filter(|open_file| open_file.fd_kind == Some(AutonomousSystemDiagnosticsFdKind::Socket))
        .filter_map(|open_file| open_file.local_port)
        .collect::<Vec<_>>();
    ports.sort_unstable();
    ports.dedup();
    row.port_count = Some(ports.len() as u32);
    if !ports.is_empty() {
        row.platform.insert(
            "ports".into(),
            ports
                .iter()
                .take(20)
                .map(u16::to_string)
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    if ports.len() > 20 {
        row.platform.insert("ports_truncated".into(), "true".into());
    }
}

#[cfg(target_os = "macos")]
fn platform_process_sample(
    runtime: &AutonomousToolRuntime,
    pid: u32,
    duration_ms: u64,
    interval_ms: u64,
    artifact_path: &Path,
) -> CommandResult<ProcessSampleRunResult> {
    macos_process_sample(runtime, pid, duration_ms, interval_ms, artifact_path)
}

#[cfg(not(target_os = "macos"))]
fn platform_process_sample(
    _runtime: &AutonomousToolRuntime,
    _pid: u32,
    _duration_ms: u64,
    _interval_ms: u64,
    _artifact_path: &Path,
) -> CommandResult<ProcessSampleRunResult> {
    Err(CommandError::user_fixable(
        "system_diagnostics_process_sample_unsupported",
        "Xero process_sample diagnostics are currently implemented for macOS `sample` only.",
    ))
}

#[cfg(target_os = "macos")]
fn macos_process_sample(
    runtime: &AutonomousToolRuntime,
    pid: u32,
    duration_ms: u64,
    interval_ms: u64,
    artifact_path: &Path,
) -> CommandResult<ProcessSampleRunResult> {
    let duration_seconds = duration_ms.div_ceil(1000).max(1).to_string();
    let interval = interval_ms.max(1).to_string();
    let started = Instant::now();
    let timeout =
        Duration::from_millis(duration_ms.saturating_add(PROCESS_SAMPLE_TIMEOUT_GRACE_MS));
    let mut child = Command::new("/usr/bin/sample")
        .args([
            pid.to_string(),
            duration_seconds,
            interval,
            "-mayDie".into(),
            "-file".into(),
            artifact_path.to_string_lossy().into_owned(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            CommandError::system_fault(
                "system_diagnostics_process_sample_failed",
                format!("Xero could not start macOS sample for PID {pid}: {error}"),
            )
        })?;

    let mut timed_out = false;
    loop {
        if let Some(_status) = child.try_wait().map_err(|error| {
            CommandError::system_fault(
                "system_diagnostics_process_sample_failed",
                format!("Xero could not poll macOS sample for PID {pid}: {error}"),
            )
        })? {
            break;
        }
        if runtime.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(cancelled_error());
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            let _ = child.kill();
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    let output = child.wait_with_output().map_err(|error| {
        CommandError::system_fault(
            "system_diagnostics_process_sample_failed",
            format!("Xero could not reap macOS sample for PID {pid}: {error}"),
        )
    })?;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let stderr_excerpt = (!stderr.trim().is_empty())
        .then(|| truncate_text_to_chars(stderr.trim(), MAX_SYSTEM_LOG_MESSAGE_CHARS));
    let mut diagnostics = Vec::new();
    if !artifact_path.exists() {
        diagnostics.push(diagnostic(
            "system_diagnostics_process_sample_artifact_missing",
            "macOS sample did not create the requested artifact path.",
        ));
    }
    Ok(ProcessSampleRunResult {
        status_success: output.status.success() && !timed_out,
        timed_out,
        exit_status: output.status.to_string(),
        wall_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        stderr,
        stderr_excerpt,
        diagnostics,
    })
}

#[cfg(target_os = "linux")]
fn platform_process_threads(
    pid: u32,
    request: &AutonomousSystemDiagnosticsRequest,
) -> CommandResult<DiagnosticsRowsResult> {
    linux_process_threads(pid, request).or_else(|proc_error| {
        ps_m_process_threads(pid).map(|mut result| {
            result.diagnostics.push(diagnostic(
                "system_diagnostics_proc_thread_fallback",
                format!(
                    "Linux /proc thread inspection failed, so Xero fell back to ps: {}",
                    proc_error.message
                ),
            ));
            result
        })
    })
}

#[cfg(all(unix, not(target_os = "linux")))]
fn platform_process_threads(
    pid: u32,
    _request: &AutonomousSystemDiagnosticsRequest,
) -> CommandResult<DiagnosticsRowsResult> {
    ps_m_process_threads(pid)
}

#[cfg(not(unix))]
fn platform_process_threads(
    _pid: u32,
    _request: &AutonomousSystemDiagnosticsRequest,
) -> CommandResult<DiagnosticsRowsResult> {
    Err(CommandError::user_fixable(
        "system_diagnostics_process_threads_unsupported",
        "Xero process_threads diagnostics are not supported on this platform yet.",
    ))
}

#[cfg(unix)]
fn ps_m_process_threads(pid: u32) -> CommandResult<DiagnosticsRowsResult> {
    let output = Command::new("ps")
        .args(["-M", &pid.to_string()])
        .output()
        .map_err(|error| {
            CommandError::system_fault(
                "system_diagnostics_ps_threads_failed",
                format!("Xero could not execute ps -M for PID {pid}: {error}"),
            )
        })?;
    if !output.status.success() && output.stdout.is_empty() {
        return Err(CommandError::retryable(
            "system_diagnostics_ps_threads_failed",
            format!("ps -M exited with status {} for PID {pid}.", output.status),
        ));
    }
    let result = parse_ps_m_threads(pid, String::from_utf8_lossy(&output.stdout).as_ref());
    if result.rows.is_empty() {
        return Err(CommandError::retryable(
            "system_diagnostics_process_threads_empty",
            format!("Xero could not find thread rows for PID {pid}."),
        ));
    }
    Ok(result)
}

#[cfg(any(unix, test))]
fn parse_ps_m_threads(pid: u32, text: &str) -> DiagnosticsRowsResult {
    let mut rows = Vec::new();
    let mut process_name = None;
    let mut redacted = false;

    for line in text.lines().skip(1) {
        let columns = line.split_whitespace().collect::<Vec<_>>();
        if columns.is_empty() {
            continue;
        }
        let parsed = if columns.len() >= 8
            && columns.get(1).and_then(|value| value.parse::<u32>().ok()) == Some(pid)
        {
            Some(ThreadColumns {
                cpu_percent: columns.get(3).map(|value| (*value).to_owned()),
                state: columns.get(4).map(|value| (*value).to_owned()),
                priority: columns.get(5).map(|value| (*value).to_owned()),
                system_time: columns.get(6).map(|value| (*value).to_owned()),
                user_time: columns.get(7).map(|value| (*value).to_owned()),
                command: (columns.len() > 8).then(|| columns[8..].join(" ")),
            })
        } else if columns.len() >= 6
            && columns.first().and_then(|value| value.parse::<u32>().ok()) == Some(pid)
        {
            Some(ThreadColumns {
                cpu_percent: columns.get(1).map(|value| (*value).to_owned()),
                state: columns.get(2).map(|value| (*value).to_owned()),
                priority: columns.get(3).map(|value| (*value).to_owned()),
                system_time: columns.get(4).map(|value| (*value).to_owned()),
                user_time: columns.get(5).map(|value| (*value).to_owned()),
                command: None,
            })
        } else {
            None
        };
        let Some(parsed) = parsed else {
            continue;
        };
        if process_name.is_none() {
            process_name = parsed
                .command
                .as_deref()
                .and_then(process_name_from_command);
        }
        let mut row = empty_row("process_thread", Some(pid), process_name.clone());
        row.cpu_percent = parsed.cpu_percent;
        row.state = parsed.state.map(|value| value.to_ascii_lowercase());
        row.priority = parsed.priority;
        if let Some(system_time) = parsed.system_time {
            row.platform.insert("system_time".into(), system_time);
        }
        if let Some(user_time) = parsed.user_time {
            row.platform.insert("user_time".into(), user_time);
        }
        if let Some(command) = parsed.command {
            let (command, command_redacted) = redact_text_for_diagnostics(&command, 240);
            redacted |= command_redacted;
            row.platform.insert("command".into(), command);
        }
        row.platform.insert("source".into(), "ps".into());
        row.platform
            .insert("thread_index".into(), rows.len().to_string());
        rows.push(row);
    }

    for row in &mut rows {
        if row.process_name.is_none() {
            row.process_name = process_name.clone();
        }
    }

    DiagnosticsRowsResult {
        rows,
        process_name,
        redacted,
        diagnostics: Vec::new(),
    }
}

#[cfg(any(unix, test))]
struct ThreadColumns {
    cpu_percent: Option<String>,
    state: Option<String>,
    priority: Option<String>,
    system_time: Option<String>,
    user_time: Option<String>,
    command: Option<String>,
}

#[cfg(target_os = "linux")]
fn linux_process_threads(
    pid: u32,
    request: &AutonomousSystemDiagnosticsRequest,
) -> CommandResult<DiagnosticsRowsResult> {
    let task_dir = Path::new("/proc").join(pid.to_string()).join("task");
    let entries = fs::read_dir(&task_dir).map_err(|error| {
        CommandError::retryable(
            "system_diagnostics_proc_task_failed",
            format!("Xero could not read {}: {error}", task_dir.display()),
        )
    })?;
    let process_name = linux_process_name(pid);
    let mut rows = Vec::new();
    for entry in entries.flatten() {
        let Some(thread_id) = entry
            .file_name()
            .to_str()
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        let root = entry.path();
        let stat = fs::read_to_string(root.join("stat")).unwrap_or_default();
        let mut row = empty_row("process_thread", Some(pid), process_name.clone());
        row.thread_id = Some(thread_id);
        row.process_name = fs::read_to_string(root.join("comm"))
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .or_else(|| process_name.clone());
        apply_linux_thread_stat(&mut row, &stat);
        if request.include_wait_channel {
            row.wait_channel = fs::read_to_string(root.join("wchan"))
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty() && value != "0");
        }
        row.platform.insert("source".into(), "procfs".into());
        rows.push(row);
    }
    if rows.is_empty() {
        return Err(CommandError::retryable(
            "system_diagnostics_process_threads_empty",
            format!("Xero could not find thread rows for PID {pid}."),
        ));
    }
    Ok(DiagnosticsRowsResult {
        rows,
        process_name,
        redacted: false,
        diagnostics: Vec::new(),
    })
}

#[cfg(target_os = "linux")]
fn apply_linux_thread_stat(row: &mut AutonomousSystemDiagnosticsRow, stat: &str) {
    let Some((_comm, after_comm)) = stat.rsplit_once(") ") else {
        return;
    };
    let fields = after_comm.split_whitespace().collect::<Vec<_>>();
    row.state = fields.first().map(|value| (*value).to_ascii_lowercase());
    row.priority = fields.get(15).map(|value| (*value).to_owned());
    if let Some(utime) = fields.get(11) {
        row.platform
            .insert("user_time_ticks".into(), (*utime).into());
    }
    if let Some(stime) = fields.get(12) {
        row.platform
            .insert("system_time_ticks".into(), (*stime).into());
    }
}

#[cfg(target_os = "macos")]
fn platform_system_log_query(
    request: &AutonomousSystemDiagnosticsRequest,
    limit: usize,
    last_ms: u64,
) -> CommandResult<SystemLogQueryResult> {
    macos_system_log_query(request, limit, last_ms)
}

#[cfg(not(target_os = "macos"))]
fn platform_system_log_query(
    _request: &AutonomousSystemDiagnosticsRequest,
    _limit: usize,
    _last_ms: u64,
) -> CommandResult<SystemLogQueryResult> {
    Err(CommandError::user_fixable(
        "system_diagnostics_log_query_unsupported",
        "Xero system_log_query diagnostics are only implemented for macOS unified logs.",
    ))
}

#[cfg(target_os = "macos")]
fn macos_system_log_query(
    request: &AutonomousSystemDiagnosticsRequest,
    limit: usize,
    last_ms: u64,
) -> CommandResult<SystemLogQueryResult> {
    let predicate = macos_log_predicate(request);
    let last_seconds = last_ms.div_ceil(1000).max(1).to_string() + "s";
    let output = Command::new("/usr/bin/log")
        .args([
            "show",
            "--last",
            &last_seconds,
            "--style",
            "json",
            "--predicate",
            &predicate,
            "--info",
            "--debug",
        ])
        .output()
        .map_err(|error| {
            CommandError::system_fault(
                "system_diagnostics_log_query_failed",
                format!("Xero could not execute macOS log show: {error}"),
            )
        })?;
    if !output.status.success() && output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CommandError::retryable(
            "system_diagnostics_log_query_failed",
            format!(
                "macOS log show exited with status {}. {}",
                output.status,
                stderr.trim()
            ),
        ));
    }
    parse_macos_log_json(String::from_utf8_lossy(&output.stdout).as_ref(), limit)
}

#[cfg(target_os = "macos")]
fn macos_log_predicate(request: &AutonomousSystemDiagnosticsRequest) -> String {
    let mut parts = Vec::new();
    if let Some(pid) = request.pid {
        parts.push(format!("processID == {pid}"));
    }
    if let Some(process_name) = request.process_name.as_deref() {
        parts.push(format!(
            "process == {}",
            macos_predicate_string(process_name)
        ));
    }
    if let Some(app_name) = request.app_name.as_deref() {
        parts.push(format!("process == {}", macos_predicate_string(app_name)));
    }
    if let Some(subsystem) = request.subsystem.as_deref() {
        parts.push(format!(
            "subsystem == {}",
            macos_predicate_string(subsystem)
        ));
    }
    if let Some(category) = request.category.as_deref() {
        parts.push(format!("category == {}", macos_predicate_string(category)));
    }
    if let Some(message) = request.message_contains.as_deref() {
        parts.push(format!(
            "eventMessage CONTAINS[c] {}",
            macos_predicate_string(message)
        ));
    }
    if let Some(level) = request.level {
        parts.push(format!(
            "messageType == {}",
            macos_predicate_string(macos_log_level_predicate_value(level))
        ));
    }
    if let Some(process_predicate) = request.process_predicate.as_deref() {
        parts.push(format!("({process_predicate})"));
    }
    parts.join(" AND ")
}

#[cfg(target_os = "macos")]
fn macos_predicate_string(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[cfg(target_os = "macos")]
fn macos_log_level_predicate_value(level: AutonomousSystemDiagnosticsLogLevel) -> &'static str {
    match level {
        AutonomousSystemDiagnosticsLogLevel::Debug => "debug",
        AutonomousSystemDiagnosticsLogLevel::Info => "info",
        AutonomousSystemDiagnosticsLogLevel::Notice => "default",
        AutonomousSystemDiagnosticsLogLevel::Error => "error",
        AutonomousSystemDiagnosticsLogLevel::Fault => "fault",
    }
}

#[cfg(target_os = "macos")]
fn parse_macos_log_json(text: &str, limit: usize) -> CommandResult<SystemLogQueryResult> {
    let mut value = serde_json::from_str::<JsonValue>(text).map_err(|error| {
        CommandError::system_fault(
            "system_diagnostics_log_parse_failed",
            format!("Xero could not parse macOS log JSON output: {error}"),
        )
    })?;
    let Some(entries) = value.as_array_mut() else {
        return Err(CommandError::system_fault(
            "system_diagnostics_log_parse_failed",
            "Xero expected macOS log JSON output to be an array.",
        ));
    };

    let mut rows = Vec::new();
    let mut redacted = false;
    let total = entries.len();
    for entry in entries.iter_mut() {
        redact_json_strings_for_diagnostics(entry, &mut redacted);
    }
    for entry in entries.iter().take(limit) {
        rows.push(macos_log_row(entry, &mut redacted));
    }
    let artifact_text = serde_json::to_string_pretty(&value).map_err(|error| {
        CommandError::system_fault(
            "system_diagnostics_log_parse_failed",
            format!("Xero could not serialize redacted macOS log artifact JSON: {error}"),
        )
    })?;
    Ok(SystemLogQueryResult {
        rows,
        artifact_text,
        redacted,
        truncated: total > limit,
        diagnostics: Vec::new(),
    })
}

#[cfg(target_os = "macos")]
fn macos_log_row(entry: &JsonValue, redacted: &mut bool) -> AutonomousSystemDiagnosticsRow {
    let pid = json_u64_field(entry, "processID").and_then(|value| u32::try_from(value).ok());
    let process_path = json_string_field(entry, "processImagePath");
    let process_name = json_string_field(entry, "process")
        .or_else(|| process_path.as_deref().and_then(process_name_from_command));
    let mut row = empty_row("system_log", pid, process_name);
    row.thread_id = json_u64_field(entry, "threadID");
    row.timestamp = json_string_field(entry, "timestamp");
    row.level = json_string_field(entry, "messageType").map(|value| value.to_ascii_lowercase());
    row.subsystem = json_string_field(entry, "subsystem");
    row.category = json_string_field(entry, "category");
    row.message = json_string_field(entry, "eventMessage").map(|value| {
        let (message, message_redacted) =
            redact_text_for_diagnostics(&value, MAX_SYSTEM_LOG_MESSAGE_CHARS);
        *redacted |= message_redacted;
        message
    });
    row.path = process_path.map(|path| {
        let (path, path_redacted) = redact_path_for_diagnostics(&path);
        *redacted |= path_redacted;
        path
    });
    row.platform
        .insert("source".into(), "macos_unified_log".into());
    for key in [
        "eventType",
        "senderImagePath",
        "traceID",
        "activityIdentifier",
    ] {
        if let Some(value) = json_string_field(entry, key) {
            row.platform.insert(camel_to_snake(key), value);
        }
    }
    row
}

#[cfg(target_os = "macos")]
fn platform_macos_accessibility_snapshot(
    request: &AutonomousSystemDiagnosticsRequest,
    limit: usize,
) -> CommandResult<MacosAccessibilitySnapshotResult> {
    macos_accessibility::snapshot(request, limit)
}

#[cfg(not(target_os = "macos"))]
fn platform_macos_accessibility_snapshot(
    _request: &AutonomousSystemDiagnosticsRequest,
    _limit: usize,
) -> CommandResult<MacosAccessibilitySnapshotResult> {
    Err(CommandError::user_fixable(
        "system_diagnostics_accessibility_snapshot_unsupported",
        "Xero macos_accessibility_snapshot diagnostics are only available on macOS hosts.",
    ))
}

fn json_string_field(value: &JsonValue, field: &str) -> Option<String> {
    match value.get(field)? {
        JsonValue::String(value) => Some(value.clone()),
        JsonValue::Number(value) => Some(value.to_string()),
        JsonValue::Bool(value) => Some(value.to_string()),
        _ => None,
    }
    .filter(|value| !value.is_empty())
}

fn json_u64_field(value: &JsonValue, field: &str) -> Option<u64> {
    match value.get(field)? {
        JsonValue::Number(value) => value.as_u64(),
        JsonValue::String(value) => value.parse::<u64>().ok(),
        _ => None,
    }
}

fn redact_json_strings_for_diagnostics(value: &mut JsonValue, redacted: &mut bool) {
    match value {
        JsonValue::String(text) => {
            let (next, was_redacted) = redact_text_for_diagnostics(text, usize::MAX);
            if was_redacted {
                *text = next;
                *redacted = true;
            }
        }
        JsonValue::Array(values) => {
            for value in values {
                redact_json_strings_for_diagnostics(value, redacted);
            }
        }
        JsonValue::Object(values) => {
            for value in values.values_mut() {
                redact_json_strings_for_diagnostics(value, redacted);
            }
        }
        _ => {}
    }
}

#[cfg(target_os = "macos")]
mod macos_accessibility {
    use std::{ffi::c_void, ptr};

    use core_foundation::{
        array::CFArray,
        base::{CFType, CFTypeID, CFTypeRef, TCFType},
        boolean::CFBoolean,
        number::CFNumber,
        string::{CFString, CFStringRef},
    };
    use core_graphics::geometry::{CGPoint, CGSize};
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::{NSRunningApplication, NSWorkspace};

    use super::*;

    type AXError = i32;
    type AXUIElementRef = *const c_void;
    type AXValueRef = *const c_void;

    const AX_ERROR_SUCCESS: AXError = 0;
    const AX_VALUE_CGPOINT_TYPE: i32 = 1;
    const AX_VALUE_CGSIZE_TYPE: i32 = 2;

    pub(super) fn snapshot(
        request: &AutonomousSystemDiagnosticsRequest,
        limit: usize,
    ) -> CommandResult<MacosAccessibilitySnapshotResult> {
        if !accessibility_permission_granted() {
            return Ok(MacosAccessibilitySnapshotResult {
                performed: false,
                target: diagnostics_target_from_request(request),
                rows: Vec::new(),
                truncated: false,
                redacted: false,
                diagnostics: vec![diagnostic(
                    "system_diagnostics_accessibility_permission_denied",
                    "Grant Xero Accessibility permission in System Settings > Privacy & Security > Accessibility, then retry the approved diagnostics action.",
                )],
            });
        }

        let targets = resolve_targets(request)?;
        let Some(first_target) = targets.first().cloned() else {
            return Err(CommandError::user_fixable(
                "system_diagnostics_accessibility_target_not_found",
                "Xero could not find a running macOS app or window matching the Accessibility snapshot target.",
            ));
        };

        let attributes = requested_attributes(request);
        let mut context = SnapshotContext {
            rows: Vec::new(),
            limit,
            truncated: false,
            redacted: false,
            diagnostics: Vec::new(),
            include_children: request.include_children,
            max_depth: request.max_depth.unwrap_or({
                if request.include_children {
                    2
                } else {
                    0
                }
            }),
            attributes,
        };

        for target in targets {
            if context.is_full() {
                break;
            }
            let Some(app_element) = AxElement::application(target.pid) else {
                context.diagnostics.push(diagnostic(
                    "system_diagnostics_accessibility_app_unavailable",
                    format!(
                        "Xero could not create an Accessibility application reference for PID {}.",
                        target.pid
                    ),
                ));
                continue;
            };

            let mut app_row =
                base_accessibility_row("macos_accessibility_app", &app_element, &target, 0);
            app_row
                .platform
                .insert("frontmost".into(), target.active.to_string());
            app_row
                .platform
                .insert("hidden".into(), target.hidden.to_string());
            if let Some(bundle_id) = target.bundle_id.as_deref() {
                app_row
                    .platform
                    .insert("bundle_id".into(), bundle_id.into());
            }
            apply_accessibility_attributes(
                &mut app_row,
                &app_element,
                &context.attributes,
                &mut context.redacted,
            );
            finalize_accessibility_state(&mut app_row);
            context.push(app_row);

            let windows = target_windows(&app_element, request.focused_only);
            if windows.is_empty() {
                context.diagnostics.push(diagnostic(
                    "system_diagnostics_accessibility_windows_empty",
                    format!(
                        "macOS Accessibility returned no window references for PID {}.",
                        target.pid
                    ),
                ));
                continue;
            }
            let mut matched_window = false;
            for (index, window) in windows.into_iter().enumerate() {
                if context.is_full() {
                    break;
                }
                let mut window_row =
                    base_accessibility_row("macos_accessibility_window", &window, &target, 0);
                window_row
                    .platform
                    .insert("window_index".into(), index.to_string());
                if let Some(window_id) = target.window_id {
                    window_row
                        .platform
                        .insert("requested_window_id".into(), window_id.to_string());
                }
                apply_accessibility_attributes(
                    &mut window_row,
                    &window,
                    &context.attributes,
                    &mut context.redacted,
                );
                finalize_accessibility_state(&mut window_row);
                if !target.window_matches(&window_row) {
                    continue;
                }
                matched_window = true;
                context.push(window_row);
                snapshot_children(&mut context, &window, &target, 1);
            }
            if target.window_id.is_some() && !matched_window {
                context.diagnostics.push(diagnostic(
                    "system_diagnostics_accessibility_window_match_approximate",
                    "macOS Accessibility does not expose stable window ids; Xero resolved the target process but could not match an AX window by title or bounds.",
                ));
            }
        }

        Ok(MacosAccessibilitySnapshotResult {
            performed: true,
            target: AutonomousSystemDiagnosticsTarget {
                pid: Some(first_target.pid),
                process_name: first_target.process_name.clone(),
                bundle_id: first_target.bundle_id.clone(),
                app_name: first_target.app_name.clone(),
                window_id: first_target.window_id,
            },
            rows: context.rows,
            truncated: context.truncated,
            redacted: context.redacted,
            diagnostics: context.diagnostics,
        })
    }

    struct SnapshotContext {
        rows: Vec<AutonomousSystemDiagnosticsRow>,
        limit: usize,
        truncated: bool,
        redacted: bool,
        diagnostics: Vec<AutonomousSystemDiagnosticsDiagnostic>,
        include_children: bool,
        max_depth: usize,
        attributes: Vec<String>,
    }

    impl SnapshotContext {
        fn is_full(&self) -> bool {
            self.rows.len() >= self.limit
        }

        fn push(&mut self, row: AutonomousSystemDiagnosticsRow) {
            if self.is_full() {
                self.truncated = true;
                return;
            }
            self.rows.push(row);
        }
    }

    #[derive(Debug, Clone)]
    struct ResolvedTarget {
        pid: u32,
        process_name: Option<String>,
        bundle_id: Option<String>,
        app_name: Option<String>,
        active: bool,
        hidden: bool,
        window_id: Option<u32>,
        window_title: Option<String>,
        bounds: Option<ResolvedWindowBounds>,
    }

    #[derive(Debug, Clone, Copy)]
    struct ResolvedWindowBounds {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    }

    impl ResolvedTarget {
        fn window_matches(&self, row: &AutonomousSystemDiagnosticsRow) -> bool {
            if self.window_id.is_none() {
                return true;
            }
            if let Some(title) = self
                .window_title
                .as_deref()
                .filter(|title| !title.is_empty())
            {
                if row
                    .platform
                    .get("title")
                    .is_some_and(|value| value == title)
                {
                    return true;
                }
            }
            let Some(bounds) = self.bounds else {
                return self.window_title.is_none();
            };
            let Some(x) = row
                .platform
                .get("x")
                .and_then(|value| value.parse::<i32>().ok())
            else {
                return false;
            };
            let Some(y) = row
                .platform
                .get("y")
                .and_then(|value| value.parse::<i32>().ok())
            else {
                return false;
            };
            let Some(width) = row
                .platform
                .get("width")
                .and_then(|value| value.parse::<u32>().ok())
            else {
                return false;
            };
            let Some(height) = row
                .platform
                .get("height")
                .and_then(|value| value.parse::<u32>().ok())
            else {
                return false;
            };
            (x - bounds.x).abs() <= 2
                && (y - bounds.y).abs() <= 2
                && width.abs_diff(bounds.width) <= 2
                && height.abs_diff(bounds.height) <= 2
        }
    }

    #[derive(Clone)]
    struct AxElement(CFType);

    impl AxElement {
        fn application(pid: u32) -> Option<Self> {
            unsafe {
                let raw = AXUIElementCreateApplication(pid as libc::pid_t);
                (!raw.is_null()).then(|| Self(CFType::wrap_under_create_rule(raw as CFTypeRef)))
            }
        }

        fn from_cf(value: CFType) -> Option<Self> {
            (value.type_of() == ax_ui_element_type_id()).then_some(Self(value))
        }

        fn as_ref(&self) -> AXUIElementRef {
            self.0.as_CFTypeRef() as AXUIElementRef
        }
    }

    fn resolve_targets(
        request: &AutonomousSystemDiagnosticsRequest,
    ) -> CommandResult<Vec<ResolvedTarget>> {
        if let Some(window_id) = request.window_id {
            return resolve_window_target(window_id, request);
        }

        let mut targets = running_apps()
            .into_iter()
            .filter(|target| target_matches_request(target, request))
            .collect::<Vec<_>>();
        if targets.is_empty() {
            if let Some(pid) = request.pid {
                targets.push(ResolvedTarget {
                    pid,
                    process_name: request.process_name.clone(),
                    bundle_id: request.bundle_id.clone(),
                    app_name: request
                        .app_name
                        .clone()
                        .or_else(|| request.process_name.clone()),
                    active: false,
                    hidden: false,
                    window_id: None,
                    window_title: None,
                    bounds: None,
                });
            }
        }
        Ok(targets)
    }

    fn resolve_window_target(
        window_id: u32,
        request: &AutonomousSystemDiagnosticsRequest,
    ) -> CommandResult<Vec<ResolvedTarget>> {
        let windows = xcap::Window::all().map_err(|error| {
            CommandError::system_fault(
                "system_diagnostics_accessibility_window_list_failed",
                format!(
                    "Xero could not enumerate macOS windows for Accessibility targeting: {error}"
                ),
            )
        })?;
        let app_summaries = running_apps();
        for window in windows {
            let Ok(id) = window.id() else {
                continue;
            };
            if id != window_id {
                continue;
            }
            let Some(pid) = window.pid().ok() else {
                return Err(CommandError::user_fixable(
                    "system_diagnostics_accessibility_window_pid_unavailable",
                    format!("Xero found macOS window `{window_id}` but could not resolve its process id."),
                ));
            };
            let mut target = app_summaries
                .iter()
                .find(|target| target.pid == pid)
                .cloned()
                .unwrap_or_else(|| ResolvedTarget {
                    pid,
                    process_name: window.app_name().ok(),
                    bundle_id: request.bundle_id.clone(),
                    app_name: request.app_name.clone(),
                    active: false,
                    hidden: false,
                    window_id: None,
                    window_title: None,
                    bounds: None,
                });
            target.window_id = Some(window_id);
            target.window_title = window.title().ok();
            target.bounds = Some(ResolvedWindowBounds {
                x: window.x().unwrap_or_default(),
                y: window.y().unwrap_or_default(),
                width: window.width().unwrap_or_default(),
                height: window.height().unwrap_or_default(),
            });
            return Ok(vec![target]);
        }
        Err(CommandError::user_fixable(
            "system_diagnostics_accessibility_window_not_found",
            format!("Xero could not find macOS window `{window_id}` for Accessibility targeting."),
        ))
    }

    fn running_apps() -> Vec<ResolvedTarget> {
        autoreleasepool(|_| {
            let workspace = NSWorkspace::sharedWorkspace();
            workspace
                .runningApplications()
                .iter()
                .filter_map(|app| app_target(&app))
                .collect()
        })
    }

    fn app_target(app: &NSRunningApplication) -> Option<ResolvedTarget> {
        let pid = normalize_pid(app.processIdentifier())?;
        let app_name = app
            .localizedName()
            .map(|name| name.to_string())
            .filter(|name| !name.trim().is_empty());
        Some(ResolvedTarget {
            pid,
            process_name: app_name.clone(),
            bundle_id: app
                .bundleIdentifier()
                .map(|bundle_id| bundle_id.to_string()),
            app_name,
            active: app.isActive(),
            hidden: app.isHidden(),
            window_id: None,
            window_title: None,
            bounds: None,
        })
    }

    fn target_matches_request(
        target: &ResolvedTarget,
        request: &AutonomousSystemDiagnosticsRequest,
    ) -> bool {
        if request.focused_only
            && request.pid.is_none()
            && request.process_name.is_none()
            && request.bundle_id.is_none()
            && request.app_name.is_none()
            && !target.active
        {
            return false;
        }
        if let Some(pid) = request.pid {
            if target.pid != pid {
                return false;
            }
        }
        if let Some(bundle_id) = request.bundle_id.as_deref() {
            if target.bundle_id.as_deref() != Some(bundle_id) {
                return false;
            }
        }
        if let Some(app_name) = request.app_name.as_deref() {
            if !target
                .app_name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(app_name))
            {
                return false;
            }
        }
        if let Some(process_name) = request.process_name.as_deref() {
            if !target
                .process_name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(process_name))
            {
                return false;
            }
        }
        true
    }

    fn base_accessibility_row(
        row_type: &str,
        element: &AxElement,
        target: &ResolvedTarget,
        depth: usize,
    ) -> AutonomousSystemDiagnosticsRow {
        let pid = element_pid(element).or(Some(target.pid));
        let mut row = empty_row(row_type, pid, target.process_name.clone());
        row.platform.insert("source".into(), "macos_ax".into());
        row.platform.insert("depth".into(), depth.to_string());
        row
    }

    fn requested_attributes(request: &AutonomousSystemDiagnosticsRequest) -> Vec<String> {
        let mut attributes = [
            "AXRole",
            "AXSubrole",
            "AXRoleDescription",
            "AXTitle",
            "AXDescription",
            "AXIdentifier",
            "AXFocused",
            "AXMain",
            "AXMinimized",
            "AXHidden",
            "AXEnabled",
            "AXSelected",
            "AXSelectedText",
            "AXValue",
            "AXPosition",
            "AXSize",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        for attribute in &request.attributes {
            let normalized = normalize_attribute_name(attribute);
            if !attributes.iter().any(|value| value == &normalized) {
                attributes.push(normalized);
            }
        }
        attributes
    }

    fn target_windows(app: &AxElement, focused_only: bool) -> Vec<AxElement> {
        if focused_only {
            if let Some(window) = ax_element_attribute(app, "AXFocusedWindow") {
                return vec![window];
            }
        }
        ax_element_array_attribute(app, "AXWindows")
    }

    fn snapshot_children(
        context: &mut SnapshotContext,
        element: &AxElement,
        target: &ResolvedTarget,
        depth: usize,
    ) {
        if !context.include_children || depth > context.max_depth || context.is_full() {
            return;
        }
        let children = ax_element_array_attribute(element, "AXChildren");
        for (index, child) in children.into_iter().enumerate() {
            if context.is_full() {
                break;
            }
            let mut row =
                base_accessibility_row("macos_accessibility_element", &child, target, depth);
            row.platform.insert("child_index".into(), index.to_string());
            apply_accessibility_attributes(
                &mut row,
                &child,
                &context.attributes,
                &mut context.redacted,
            );
            finalize_accessibility_state(&mut row);
            context.push(row);
            snapshot_children(context, &child, target, depth + 1);
        }
    }

    fn apply_accessibility_attributes(
        row: &mut AutonomousSystemDiagnosticsRow,
        element: &AxElement,
        attributes: &[String],
        redacted: &mut bool,
    ) {
        for attribute in attributes {
            apply_accessibility_attribute(row, element, attribute, redacted);
        }
    }

    fn apply_accessibility_attribute(
        row: &mut AutonomousSystemDiagnosticsRow,
        element: &AxElement,
        attribute: &str,
        redacted: &mut bool,
    ) {
        match attribute {
            "AXPosition" => {
                if let Some(point) = ax_point_attribute(element, attribute) {
                    row.platform.insert("x".into(), point.x.round().to_string());
                    row.platform.insert("y".into(), point.y.round().to_string());
                }
            }
            "AXSize" => {
                if let Some(size) = ax_size_attribute(element, attribute) {
                    row.platform
                        .insert("width".into(), size.width.round().to_string());
                    row.platform
                        .insert("height".into(), size.height.round().to_string());
                }
            }
            _ => {
                let Some(value) = ax_attribute(element, attribute) else {
                    return;
                };
                let Some(text) = cf_value_summary(&value) else {
                    return;
                };
                let (text, was_redacted) = redact_text_for_diagnostics(&text, 240);
                *redacted |= was_redacted;
                let key = accessibility_platform_key(attribute);
                if key == "title" {
                    row.message = Some(text.clone());
                }
                row.platform.insert(key, text);
            }
        }
    }

    fn finalize_accessibility_state(row: &mut AutonomousSystemDiagnosticsRow) {
        let focused = bool_platform_value(row, "focused").unwrap_or(false);
        let hidden = bool_platform_value(row, "hidden").unwrap_or(false);
        let minimized = bool_platform_value(row, "minimized").unwrap_or(false);
        let visible = !(hidden || minimized);
        row.platform.insert("visible".into(), visible.to_string());
        row.state = Some(
            if focused {
                "focused"
            } else if visible {
                "visible"
            } else {
                "hidden"
            }
            .into(),
        );
    }

    fn bool_platform_value(row: &AutonomousSystemDiagnosticsRow, key: &str) -> Option<bool> {
        row.platform
            .get(key)
            .and_then(|value| value.parse::<bool>().ok())
    }

    fn ax_attribute(element: &AxElement, attribute: &str) -> Option<CFType> {
        let attribute = CFString::new(attribute);
        let mut value: CFTypeRef = ptr::null();
        let status = unsafe {
            AXUIElementCopyAttributeValue(
                element.as_ref(),
                attribute.as_concrete_TypeRef(),
                &mut value,
            )
        };
        (status == AX_ERROR_SUCCESS && !value.is_null())
            .then(|| unsafe { CFType::wrap_under_create_rule(value) })
    }

    fn ax_element_attribute(element: &AxElement, attribute: &str) -> Option<AxElement> {
        AxElement::from_cf(ax_attribute(element, attribute)?)
    }

    fn ax_element_array_attribute(element: &AxElement, attribute: &str) -> Vec<AxElement> {
        let Some(value) = ax_attribute(element, attribute) else {
            return Vec::new();
        };
        let Some(array) = value.downcast::<CFArray>() else {
            return Vec::new();
        };
        array
            .get_all_values()
            .into_iter()
            .filter_map(|value| {
                if value.is_null() {
                    return None;
                }
                let cf_type = unsafe { CFType::wrap_under_get_rule(value as CFTypeRef) };
                AxElement::from_cf(cf_type)
            })
            .collect()
    }

    fn ax_point_attribute(element: &AxElement, attribute: &str) -> Option<CGPoint> {
        let value = ax_attribute(element, attribute)?;
        if value.type_of() != ax_value_type_id() {
            return None;
        }
        if unsafe { AXValueGetType(value.as_CFTypeRef() as AXValueRef) } != AX_VALUE_CGPOINT_TYPE {
            return None;
        }
        let mut point = CGPoint::default();
        let ok = unsafe {
            AXValueGetValue(
                value.as_CFTypeRef() as AXValueRef,
                AX_VALUE_CGPOINT_TYPE,
                &mut point as *mut CGPoint as *mut c_void,
            )
        };
        ok.then_some(point)
    }

    fn ax_size_attribute(element: &AxElement, attribute: &str) -> Option<CGSize> {
        let value = ax_attribute(element, attribute)?;
        if value.type_of() != ax_value_type_id() {
            return None;
        }
        if unsafe { AXValueGetType(value.as_CFTypeRef() as AXValueRef) } != AX_VALUE_CGSIZE_TYPE {
            return None;
        }
        let mut size = CGSize::default();
        let ok = unsafe {
            AXValueGetValue(
                value.as_CFTypeRef() as AXValueRef,
                AX_VALUE_CGSIZE_TYPE,
                &mut size as *mut CGSize as *mut c_void,
            )
        };
        ok.then_some(size)
    }

    fn cf_value_summary(value: &CFType) -> Option<String> {
        if let Some(value) = value.downcast::<CFString>() {
            return Some(value.to_string());
        }
        if let Some(value) = value.downcast::<CFBoolean>() {
            return Some(bool::from(value).to_string());
        }
        if let Some(value) = value.downcast::<CFNumber>() {
            if let Some(integer) = value.to_i64() {
                return Some(integer.to_string());
            }
            if let Some(float) = value.to_f64() {
                return Some(float.to_string());
            }
        }
        if let Some(array) = value.downcast::<CFArray>() {
            return Some(format!("array({})", array.len()));
        }
        if value.type_of() == ax_ui_element_type_id() {
            return Some("AXUIElement".into());
        }
        if value.type_of() == ax_value_type_id() {
            return Some("AXValue".into());
        }
        Some(format!("{value:?}"))
    }

    fn element_pid(element: &AxElement) -> Option<u32> {
        let mut pid: libc::pid_t = 0;
        let status = unsafe { AXUIElementGetPid(element.as_ref(), &mut pid) };
        (status == AX_ERROR_SUCCESS && pid > 0).then_some(pid as u32)
    }

    fn normalize_pid(pid: libc::pid_t) -> Option<u32> {
        (pid > 0).then_some(pid as u32)
    }

    fn normalize_attribute_name(attribute: &str) -> String {
        let trimmed = attribute.trim();
        if trimmed.starts_with("AX") {
            return trimmed.into();
        }
        match trimmed {
            "role" => "AXRole",
            "subrole" => "AXSubrole",
            "roleDescription" | "role_description" => "AXRoleDescription",
            "title" => "AXTitle",
            "description" => "AXDescription",
            "identifier" => "AXIdentifier",
            "focused" => "AXFocused",
            "main" => "AXMain",
            "minimized" => "AXMinimized",
            "hidden" => "AXHidden",
            "enabled" => "AXEnabled",
            "selected" => "AXSelected",
            "selectedText" | "selected_text" => "AXSelectedText",
            "value" => "AXValue",
            "position" => "AXPosition",
            "size" => "AXSize",
            other => other,
        }
        .into()
    }

    fn accessibility_platform_key(attribute: &str) -> String {
        camel_to_snake(attribute.trim_start_matches("AX"))
    }

    fn accessibility_permission_granted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    fn ax_ui_element_type_id() -> CFTypeID {
        unsafe { AXUIElementGetTypeID() }
    }

    fn ax_value_type_id() -> CFTypeID {
        unsafe { AXValueGetTypeID() }
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXUIElementCreateApplication(pid: libc::pid_t) -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
        fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut libc::pid_t) -> AXError;
        fn AXUIElementGetTypeID() -> CFTypeID;
        fn AXValueGetTypeID() -> CFTypeID;
        fn AXValueGetType(value: AXValueRef) -> i32;
        fn AXValueGetValue(value: AXValueRef, value_type: i32, value: *mut c_void) -> bool;
    }
}

fn process_name_from_command(command: &str) -> Option<String> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    let first = command.split_whitespace().next().unwrap_or(command);
    Path::new(first)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .or_else(|| Some(first.to_owned()))
}

fn process_path_from_command(command: &str) -> Option<String> {
    let first = command.split_whitespace().next()?;
    first.starts_with('/').then(|| first.to_owned())
}

fn redact_text_for_diagnostics(value: &str, max_chars: usize) -> (String, bool) {
    if find_prohibited_persistence_content(value).is_some() {
        return ("[REDACTED]".into(), true);
    }
    (truncate_text_to_chars(value, max_chars), false)
}

fn truncate_text_to_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.into();
    }
    let mut out = value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    out.push_str("...");
    out
}

fn truncate_text_to_bytes(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.into(), false);
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    (value[..end].into(), true)
}

fn read_text_file_prefix(path: &Path, max_bytes: usize) -> CommandResult<(String, bool)> {
    let mut file = fs::File::open(path).map_err(|error| {
        CommandError::system_fault(
            "system_diagnostics_artifact_failed",
            format!(
                "Xero could not read diagnostics artifact {}: {error}",
                path.display()
            ),
        )
    })?;
    let mut bytes = Vec::new();
    let limit = u64::try_from(max_bytes).unwrap_or(u64::MAX);
    file.by_ref()
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            CommandError::system_fault(
                "system_diagnostics_artifact_failed",
                format!(
                    "Xero could not read diagnostics artifact {}: {error}",
                    path.display()
                ),
            )
        })?;
    let truncated = fs::metadata(path)
        .map(|metadata| metadata.len() > bytes.len() as u64)
        .unwrap_or(false);
    Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
}

fn write_text_artifact(path: &Path, text: &str) -> CommandResult<()> {
    fs::write(path, text.as_bytes()).map_err(|error| {
        CommandError::system_fault(
            "system_diagnostics_artifact_failed",
            format!(
                "Xero could not write diagnostics artifact {}: {error}",
                path.display()
            ),
        )
    })
}

fn sample_profile_excerpt(text: &str) -> Option<String> {
    let excerpt = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(8)
        .collect::<Vec<_>>()
        .join("\n");
    (!excerpt.is_empty()).then(|| truncate_text_to_chars(&excerpt, 800))
}

fn diagnostics_artifact_path(root: &Path, prefix: &str, extension: &str) -> CommandResult<PathBuf> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            CommandError::system_fault(
                "system_diagnostics_artifact_failed",
                format!("Xero could not timestamp the diagnostics artifact: {error}"),
            )
        })?
        .as_millis();
    Ok(root.join(format!("{prefix}-{millis}.{extension}")))
}

fn camel_to_snake(value: &str) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(target_os = "linux")]
fn platform_process_open_files(pid: u32) -> CommandResult<ProcessOpenFilesResult> {
    match linux_process_open_files(pid) {
        Ok(result) => Ok(result),
        Err(proc_error) => match lsof_process_open_files(pid) {
            Ok(mut result) => {
                result.diagnostics.push(diagnostic(
                    "system_diagnostics_proc_fallback",
                    format!(
                        "Linux /proc inspection failed, so Xero fell back to lsof: {}",
                        proc_error.message
                    ),
                ));
                Ok(result)
            }
            Err(_) => Err(proc_error),
        },
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
fn platform_process_open_files(pid: u32) -> CommandResult<ProcessOpenFilesResult> {
    lsof_process_open_files(pid)
}

#[cfg(not(unix))]
fn platform_process_open_files(_pid: u32) -> CommandResult<ProcessOpenFilesResult> {
    Err(CommandError::user_fixable(
        "system_diagnostics_process_open_files_unsupported",
        "Xero process_open_files diagnostics are not supported on this platform yet.",
    ))
}

#[cfg(unix)]
fn lsof_process_open_files(pid: u32) -> CommandResult<ProcessOpenFilesResult> {
    let output = Command::new("lsof")
        .args(["-nP", "-p", &pid.to_string(), "-F", "0pcfatnPT"])
        .output()
        .map_err(|error| {
            CommandError::system_fault(
                "system_diagnostics_lsof_failed",
                format!("Xero could not execute lsof for PID {pid}: {error}"),
            )
        })?;
    if !output.status.success() && output.stdout.is_empty() {
        return Err(CommandError::retryable(
            "system_diagnostics_lsof_failed",
            format!("lsof exited with status {} for PID {pid}.", output.status),
        ));
    }

    Ok(parse_lsof_field_output(
        pid,
        String::from_utf8_lossy(&output.stdout).as_ref(),
    ))
}

#[cfg(any(unix, test))]
fn parse_lsof_field_output(pid: u32, text: &str) -> ProcessOpenFilesResult {
    let mut rows = Vec::new();
    let mut current = None;
    let mut process_name = None;
    let mut redacted = false;

    for raw_item in text.split('\0') {
        let item = raw_item.trim_start_matches('\n').trim_end_matches('\n');
        if item.is_empty() {
            continue;
        }
        let (tag, value) = item.split_at(1);
        match tag {
            "p" => {
                flush_lsof_row(&mut current, &mut rows, &mut redacted);
                process_name = None;
            }
            "c" => {
                process_name = Some(value.to_owned());
                if let Some(row) = current.as_mut() {
                    row.process_name = Some(value.to_owned());
                }
            }
            "f" => {
                flush_lsof_row(&mut current, &mut rows, &mut redacted);
                let (fd, access) = split_fd_and_access(value);
                current = Some(base_row(
                    pid,
                    process_name.clone(),
                    Some(fd),
                    access.map(str::to_owned),
                    "lsof",
                ));
            }
            "a" => {
                if let Some(row) = current.as_mut() {
                    let value = value.trim();
                    if !value.is_empty() {
                        row.access = Some(access_label(value).unwrap_or(value).to_owned());
                    }
                }
            }
            "t" => {
                if let Some(row) = current.as_mut() {
                    row.file_type = Some(value.to_owned());
                }
            }
            "P" => {
                if let Some(row) = current.as_mut() {
                    row.protocol = Some(value.to_ascii_lowercase());
                }
            }
            "n" => {
                if let Some(row) = current.as_mut() {
                    apply_lsof_name(row, value);
                }
            }
            "T" => {
                if let Some(row) = current.as_mut() {
                    apply_lsof_tcp_field(row, value);
                }
            }
            _ => {}
        }
    }
    flush_lsof_row(&mut current, &mut rows, &mut redacted);

    ProcessOpenFilesResult {
        rows,
        process_name,
        redacted,
        diagnostics: Vec::new(),
    }
}

fn base_row(
    pid: u32,
    process_name: Option<String>,
    fd: Option<String>,
    access: Option<String>,
    source: &str,
) -> AutonomousSystemDiagnosticsRow {
    let mut platform = BTreeMap::new();
    platform.insert("source".into(), source.into());
    AutonomousSystemDiagnosticsRow {
        row_type: "process_open_file".into(),
        pid: Some(pid),
        parent_pid: None,
        process_name,
        thread_id: None,
        fd,
        fd_kind: None,
        access,
        file_type: None,
        protocol: None,
        local_addr: None,
        local_port: None,
        remote_addr: None,
        remote_port: None,
        state: None,
        path: None,
        cpu_percent: None,
        cpu_time: None,
        memory_bytes: None,
        virtual_memory_bytes: None,
        thread_count: None,
        port_count: None,
        priority: None,
        wait_channel: None,
        timestamp: None,
        level: None,
        subsystem: None,
        category: None,
        message: None,
        deleted: false,
        platform,
    }
}

#[cfg(any(unix, test))]
fn flush_lsof_row(
    current: &mut Option<AutonomousSystemDiagnosticsRow>,
    rows: &mut Vec<AutonomousSystemDiagnosticsRow>,
    redacted: &mut bool,
) {
    let Some(mut row) = current.take() else {
        return;
    };
    finalize_open_file_row(&mut row, redacted);
    rows.push(row);
}

#[cfg(any(unix, test))]
fn apply_lsof_name(row: &mut AutonomousSystemDiagnosticsRow, value: &str) {
    if is_socketish(row) {
        apply_socket_name(row, value);
        return;
    }
    let (path, deleted) = strip_deleted_marker(value);
    row.deleted |= deleted;
    row.path = Some(path.to_owned());
}

fn apply_socket_name(row: &mut AutonomousSystemDiagnosticsRow, value: &str) {
    row.fd_kind = Some(AutonomousSystemDiagnosticsFdKind::Socket);
    if let Some((state, without_state)) = split_socket_state(value) {
        row.state = Some(state.to_ascii_lowercase());
        parse_socket_endpoint_pair(row, without_state);
    } else {
        parse_socket_endpoint_pair(row, value);
    }
}

#[cfg(any(unix, test))]
fn apply_lsof_tcp_field(row: &mut AutonomousSystemDiagnosticsRow, value: &str) {
    if let Some(state) = value.strip_prefix("ST=") {
        row.state = Some(state.to_ascii_lowercase());
    } else if let Some((key, raw)) = value.split_once('=') {
        row.platform
            .insert(format!("tcp_{}", key.to_ascii_lowercase()), raw.to_owned());
    }
}

fn finalize_open_file_row(row: &mut AutonomousSystemDiagnosticsRow, redacted: &mut bool) {
    if row.fd_kind.is_none() {
        row.fd_kind = Some(classify_fd_kind(row));
    }
    if row.deleted && row.fd_kind != Some(AutonomousSystemDiagnosticsFdKind::Socket) {
        row.fd_kind = Some(AutonomousSystemDiagnosticsFdKind::Deleted);
    }
    if let Some(path) = row.path.take() {
        let (path, path_redacted) = redact_path_for_diagnostics(&path);
        row.path = Some(path);
        *redacted |= path_redacted;
    }
}

fn classify_fd_kind(row: &AutonomousSystemDiagnosticsRow) -> AutonomousSystemDiagnosticsFdKind {
    let fd = row.fd.as_deref().unwrap_or_default();
    if fd == "cwd" {
        return AutonomousSystemDiagnosticsFdKind::Cwd;
    }
    if fd == "txt" {
        return AutonomousSystemDiagnosticsFdKind::Executable;
    }
    if is_socketish(row) {
        return AutonomousSystemDiagnosticsFdKind::Socket;
    }
    let file_type = row.file_type.as_deref().unwrap_or_default();
    match file_type {
        "REG" => AutonomousSystemDiagnosticsFdKind::File,
        "DIR" => AutonomousSystemDiagnosticsFdKind::Directory,
        "FIFO" | "PIPE" => AutonomousSystemDiagnosticsFdKind::Pipe,
        "CHR" | "BLK" => AutonomousSystemDiagnosticsFdKind::Device,
        _ => AutonomousSystemDiagnosticsFdKind::Other,
    }
}

fn is_socketish(row: &AutonomousSystemDiagnosticsRow) -> bool {
    row.protocol.is_some()
        || matches!(
            row.file_type.as_deref(),
            Some("IPv4" | "IPv6" | "unix" | "sock" | "SOCK")
        )
}

fn split_fd_and_access(value: &str) -> (String, Option<&'static str>) {
    let mut chars = value.chars();
    let Some(last) = chars.next_back() else {
        return (value.into(), None);
    };
    if matches!(last, 'r' | 'w' | 'u')
        && value[..value.len() - last.len_utf8()]
            .chars()
            .any(|ch| ch.is_ascii_digit())
    {
        let fd = value[..value.len() - last.len_utf8()].to_owned();
        return (fd, access_label(&last.to_string()));
    }
    (value.into(), None)
}

fn access_label(value: &str) -> Option<&'static str> {
    match value {
        "r" => Some("read"),
        "w" => Some("write"),
        "u" => Some("read_write"),
        _ => None,
    }
}

fn strip_deleted_marker(value: &str) -> (&str, bool) {
    if let Some(path) = value.strip_suffix(" (deleted)") {
        return (path, true);
    }
    if let Some(path) = value.strip_suffix(" (deleted inode)") {
        return (path, true);
    }
    (value, false)
}

fn split_socket_state(value: &str) -> Option<(String, &str)> {
    let start = value.rfind(" (")?;
    let state = value.get(start + 2..value.len().saturating_sub(1))?;
    Some((state.to_owned(), &value[..start]))
}

fn parse_socket_endpoint_pair(row: &mut AutonomousSystemDiagnosticsRow, value: &str) {
    let (local, remote) = value.split_once("->").unwrap_or((value, ""));
    if let Some((addr, port)) = parse_socket_endpoint(local.trim()) {
        row.local_addr = Some(addr);
        row.local_port = Some(port);
    } else if !local.trim().is_empty() && local.trim().starts_with('/') {
        row.path = Some(local.trim().to_owned());
    }
    if let Some((addr, port)) = parse_socket_endpoint(remote.trim()) {
        row.remote_addr = Some(addr);
        row.remote_port = Some(port);
    }
}

fn parse_socket_endpoint(value: &str) -> Option<(String, u16)> {
    if value.is_empty() {
        return None;
    }
    if let Some(end) = value.rfind("]:") {
        let addr = value[..=end]
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_owned();
        let port = value[end + 2..].parse::<u16>().ok()?;
        return Some((addr, port));
    }
    let (addr, port) = value.rsplit_once(':')?;
    let port = port.parse::<u16>().ok()?;
    Some((addr.to_owned(), port))
}

fn redact_path_for_diagnostics(value: &str) -> (String, bool) {
    if path_looks_sensitive(value) {
        ("[REDACTED]".into(), true)
    } else {
        (value.into(), false)
    }
}

fn path_looks_sensitive(value: &str) -> bool {
    if find_prohibited_persistence_content(value).is_some() {
        return true;
    }
    let normalized = value.replace('\\', "/").to_ascii_lowercase();
    let file_name = Path::new(&normalized)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(normalized.as_str());
    file_name == ".env"
        || file_name.starts_with(".env.")
        || matches!(
            file_name,
            "id_rsa"
                | "id_dsa"
                | "id_ecdsa"
                | "id_ed25519"
                | "credentials"
                | "credentials.json"
                | "secrets.json"
                | "service-account.json"
        )
        || normalized.contains("/.ssh/")
        || normalized.contains("/.aws/")
        || normalized.contains("/.gnupg/")
        || normalized.contains("private_key")
        || normalized.contains("private-key")
        || normalized.ends_with(".pem")
        || normalized.ends_with(".p12")
        || normalized.ends_with(".pfx")
}

#[cfg(target_os = "linux")]
fn linux_process_open_files(pid: u32) -> CommandResult<ProcessOpenFilesResult> {
    let proc_root = Path::new("/proc").join(pid.to_string());
    let process_name = linux_process_name(pid);
    let socket_map = linux_socket_map();
    let mut rows = Vec::new();
    let mut redacted = false;

    if let Ok(cwd) = fs::read_link(proc_root.join("cwd")) {
        let mut row = base_row(
            pid,
            process_name.clone(),
            Some("cwd".into()),
            None,
            "procfs",
        );
        row.file_type = Some("DIR".into());
        row.path = Some(cwd.to_string_lossy().into_owned());
        finalize_open_file_row(&mut row, &mut redacted);
        rows.push(row);
    }
    if let Ok(exe) = fs::read_link(proc_root.join("exe")) {
        let mut row = base_row(
            pid,
            process_name.clone(),
            Some("txt".into()),
            None,
            "procfs",
        );
        row.file_type = Some("REG".into());
        row.path = Some(exe.to_string_lossy().into_owned());
        finalize_open_file_row(&mut row, &mut redacted);
        rows.push(row);
    }

    let fd_dir = proc_root.join("fd");
    let entries = fs::read_dir(&fd_dir).map_err(|error| {
        CommandError::retryable(
            "system_diagnostics_proc_fd_failed",
            format!("Xero could not read {}: {error}", fd_dir.display()),
        )
    })?;
    for entry in entries.flatten() {
        let fd = entry.file_name().to_string_lossy().into_owned();
        let Ok(target) = fs::read_link(entry.path()) else {
            continue;
        };
        let mut row = base_row(pid, process_name.clone(), Some(fd), None, "procfs");
        apply_linux_fd_target(&mut row, &target.to_string_lossy(), &socket_map);
        finalize_open_file_row(&mut row, &mut redacted);
        rows.push(row);
    }

    Ok(ProcessOpenFilesResult {
        rows,
        process_name,
        redacted,
        diagnostics: Vec::new(),
    })
}

#[cfg(target_os = "linux")]
fn linux_process_name(pid: u32) -> Option<String> {
    fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(target_os = "linux")]
fn apply_linux_fd_target(
    row: &mut AutonomousSystemDiagnosticsRow,
    target: &str,
    socket_map: &BTreeMap<String, LinuxSocketInfo>,
) {
    if let Some(inode) = target
        .strip_prefix("socket:[")
        .and_then(|value| value.strip_suffix(']'))
    {
        row.fd_kind = Some(AutonomousSystemDiagnosticsFdKind::Socket);
        row.platform.insert("inode".into(), inode.into());
        if let Some(socket) = socket_map.get(inode) {
            row.protocol = Some(socket.protocol.clone());
            row.local_addr = Some(socket.local_addr.clone());
            row.local_port = socket.local_port;
            row.remote_addr = socket.remote_addr.clone();
            row.remote_port = socket.remote_port;
            row.state = socket.state.clone();
            row.path = socket.path.clone();
        }
        return;
    }
    if target.starts_with("pipe:[") {
        row.fd_kind = Some(AutonomousSystemDiagnosticsFdKind::Pipe);
        row.file_type = Some("PIPE".into());
        row.path = Some(target.into());
        return;
    }
    if target.starts_with("anon_inode:") {
        row.fd_kind = Some(AutonomousSystemDiagnosticsFdKind::Other);
        row.path = Some(target.into());
        return;
    }

    let (path, deleted) = strip_deleted_marker(target);
    row.deleted |= deleted;
    row.path = Some(path.into());
    row.file_type = fs::metadata(path)
        .ok()
        .map(|metadata| {
            if metadata.is_dir() {
                "DIR"
            } else if metadata.is_file() {
                "REG"
            } else {
                "OTHER"
            }
            .to_owned()
        })
        .or_else(|| row.deleted.then(|| "REG".into()));
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
struct LinuxSocketInfo {
    protocol: String,
    local_addr: String,
    local_port: Option<u16>,
    remote_addr: Option<String>,
    remote_port: Option<u16>,
    state: Option<String>,
    path: Option<String>,
}

#[cfg(target_os = "linux")]
fn linux_socket_map() -> BTreeMap<String, LinuxSocketInfo> {
    let mut sockets = BTreeMap::new();
    for (path, protocol, ipv6) in [
        ("/proc/net/tcp", "tcp", false),
        ("/proc/net/tcp6", "tcp6", true),
        ("/proc/net/udp", "udp", false),
        ("/proc/net/udp6", "udp6", true),
    ] {
        if let Ok(content) = fs::read_to_string(path) {
            parse_linux_inet_sockets(&content, protocol, ipv6, &mut sockets);
        }
    }
    if let Ok(content) = fs::read_to_string("/proc/net/unix") {
        parse_linux_unix_sockets(&content, &mut sockets);
    }
    sockets
}

#[cfg(target_os = "linux")]
fn parse_linux_inet_sockets(
    content: &str,
    protocol: &str,
    ipv6: bool,
    sockets: &mut BTreeMap<String, LinuxSocketInfo>,
) {
    for line in content.lines().skip(1) {
        let columns = line.split_whitespace().collect::<Vec<_>>();
        if columns.len() < 10 {
            continue;
        }
        let Some((local_addr, local_port)) = parse_linux_socket_addr(columns[1], ipv6) else {
            continue;
        };
        let remote = parse_linux_socket_addr(columns[2], ipv6);
        sockets.insert(
            columns[9].into(),
            LinuxSocketInfo {
                protocol: protocol.into(),
                local_addr,
                local_port: Some(local_port),
                remote_addr: remote.as_ref().map(|(addr, _port)| addr.clone()),
                remote_port: remote.map(|(_addr, port)| port),
                state: Some(linux_socket_state(columns[3]).into()),
                path: None,
            },
        );
    }
}

#[cfg(target_os = "linux")]
fn parse_linux_socket_addr(value: &str, ipv6: bool) -> Option<(String, u16)> {
    let (addr_hex, port_hex) = value.split_once(':')?;
    let port = u16::from_str_radix(port_hex, 16).ok()?;
    let addr = if ipv6 {
        linux_ipv6_addr(addr_hex)
    } else {
        linux_ipv4_addr(addr_hex)
    };
    Some((addr, port))
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
        .iter()
        .map(|segment| format!("{segment:x}"))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(target_os = "linux")]
fn linux_socket_state(value: &str) -> &'static str {
    match value {
        "01" => "established",
        "02" => "syn_sent",
        "03" => "syn_recv",
        "04" => "fin_wait1",
        "05" => "fin_wait2",
        "06" => "time_wait",
        "07" => "close",
        "08" => "close_wait",
        "09" => "last_ack",
        "0A" => "listen",
        "0B" => "closing",
        _ => "unknown",
    }
}

#[cfg(target_os = "linux")]
fn parse_linux_unix_sockets(content: &str, sockets: &mut BTreeMap<String, LinuxSocketInfo>) {
    for line in content.lines().skip(1) {
        let columns = line.split_whitespace().collect::<Vec<_>>();
        if columns.len() < 7 {
            continue;
        }
        let path = columns.get(7).map(|value| (*value).to_owned());
        sockets.insert(
            columns[6].into(),
            LinuxSocketInfo {
                protocol: "unix".into(),
                local_addr: "unix".into(),
                local_port: None,
                remote_addr: None,
                remote_port: None,
                state: Some("open".into()),
                path,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::Write, net::TcpListener, process::Command as ProcessCommand, thread, time::Duration,
    };

    use tempfile::NamedTempFile;

    use super::*;
    use crate::{
        auth::now_timestamp,
        commands::{
            RuntimeAgentIdDto, RuntimeRunActiveControlSnapshotDto, RuntimeRunApprovalModeDto,
            RuntimeRunControlStateDto,
        },
        runtime::{
            AutonomousSystemDiagnosticsFdKind, AutonomousToolAccessAction,
            AutonomousToolAccessRequest, AutonomousToolRequest,
        },
    };

    #[test]
    fn lsof_field_parser_returns_structured_file_and_socket_rows() {
        let payload = concat!(
            "p123\0",
            "ctest-proc\0",
            "fcwd\0",
            "tDIR\0",
            "n/tmp/project\0",
            "f3u\0",
            "au\0",
            "tREG\0",
            "n/tmp/example.txt\0",
            "f4u\0",
            "tIPv4\0",
            "PTCP\0",
            "n127.0.0.1:49331 (LISTEN)\0",
            "TST=LISTEN\0",
        );

        let parsed = parse_lsof_field_output(123, payload);

        assert!(parsed.rows.iter().any(|row| {
            row.fd.as_deref() == Some("3")
                && row.fd_kind == Some(AutonomousSystemDiagnosticsFdKind::File)
                && row.access.as_deref() == Some("read_write")
                && row.path.as_deref() == Some("/tmp/example.txt")
        }));
        assert!(parsed.rows.iter().any(|row| {
            row.fd.as_deref() == Some("4")
                && row.fd_kind == Some(AutonomousSystemDiagnosticsFdKind::Socket)
                && row.protocol.as_deref() == Some("tcp")
                && row.local_port == Some(49331)
                && row.state.as_deref() == Some("listen")
        }));
    }

    #[test]
    fn diagnostics_bundle_requires_preset() {
        let repo = tempfile::tempdir().expect("repo");
        let runtime = AutonomousToolRuntime::new(repo.path()).expect("runtime");
        let mut request = diagnostics_request(
            AutonomousSystemDiagnosticsAction::DiagnosticsBundle,
            std::process::id(),
        );
        request.preset = None;
        let error = runtime
            .system_diagnostics(request)
            .expect_err("bundle preset should be required");

        assert_eq!(error.code, "system_diagnostics_bundle_preset_required");
    }

    #[test]
    fn ps_resource_parser_returns_resource_snapshot_row() {
        let parsed = parse_ps_resource_snapshot(
            123,
            "  123     1 S      12.5   2048   4096 01:02   0:03.04 /usr/bin/example\n",
        )
        .expect("parsed resource snapshot");
        let row = &parsed.rows[0];

        assert_eq!(row.row_type, "process_resource_snapshot");
        assert_eq!(row.pid, Some(123));
        assert_eq!(row.parent_pid, Some(1));
        assert_eq!(row.process_name.as_deref(), Some("example"));
        assert_eq!(row.state.as_deref(), Some("s"));
        assert_eq!(row.cpu_percent.as_deref(), Some("12.5"));
        assert_eq!(row.memory_bytes, Some(2_097_152));
        assert_eq!(row.virtual_memory_bytes, Some(4_194_304));
        assert_eq!(row.cpu_time.as_deref(), Some("0:03.04"));
    }

    #[test]
    fn ps_m_parser_returns_bounded_thread_rows() {
        let payload = concat!(
            "USER   PID   TT   %CPU STAT PRI     STIME     UTIME COMMAND\n",
            "sn0w 93480   ??    0.0 S    31T   0:03.06   0:00.43 /Applications/Codex.app/Contents/Resources/codex app-server\n",
            "     93480         2.7 R    31T   0:05.48   0:09.43 \n",
        );

        let parsed = parse_ps_m_threads(93480, payload);

        assert_eq!(parsed.rows.len(), 2);
        assert_eq!(parsed.process_name.as_deref(), Some("codex"));
        assert_eq!(parsed.rows[0].row_type, "process_thread");
        assert_eq!(parsed.rows[0].cpu_percent.as_deref(), Some("0.0"));
        assert_eq!(parsed.rows[0].state.as_deref(), Some("s"));
        assert_eq!(parsed.rows[1].cpu_percent.as_deref(), Some("2.7"));
        assert_eq!(parsed.rows[1].state.as_deref(), Some("r"));
        assert_eq!(
            parsed.rows[1]
                .platform
                .get("thread_index")
                .map(String::as_str),
            Some("1")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_log_parser_returns_structured_rows() {
        let payload = r#"[{
          "timestamp": "2026-05-03 00:27:29.680215-0400",
          "messageType": "Error",
          "eventType": "logEvent",
          "subsystem": "com.example",
          "category": "diagnostics",
          "threadID": 456,
          "processImagePath": "/usr/bin/log",
          "processID": 123,
          "eventMessage": "known diagnostic message"
        }]"#;

        let parsed = parse_macos_log_json(payload, 10).expect("parsed log json");
        let row = &parsed.rows[0];

        assert_eq!(row.row_type, "system_log");
        assert_eq!(row.pid, Some(123));
        assert_eq!(row.thread_id, Some(456));
        assert_eq!(row.process_name.as_deref(), Some("log"));
        assert_eq!(row.level.as_deref(), Some("error"));
        assert_eq!(row.subsystem.as_deref(), Some("com.example"));
        assert_eq!(row.category.as_deref(), Some("diagnostics"));
        assert_eq!(row.message.as_deref(), Some("known diagnostic message"));
        assert!(!parsed.truncated);
    }

    #[test]
    fn malformed_process_open_files_request_is_rejected() {
        let request = serde_json::json!({
            "tool": "system_diagnostics",
            "input": {
                "action": "process_open_files",
                "pid": std::process::id(),
                "unexpected": true
            }
        });

        let error = serde_json::from_value::<AutonomousToolRequest>(request)
            .expect_err("unknown fields should fail");

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn tool_search_and_access_activate_system_diagnostics_for_engineer() {
        let repo = tempfile::tempdir().expect("repo");
        let runtime = test_engineer_runtime(repo.path());

        let search = runtime
            .tool_search(super::super::AutonomousToolSearchRequest {
                query: "lsof open files for process sockets".into(),
                limit: Some(10),
            })
            .expect("tool search");
        let AutonomousToolOutput::ToolSearch(search_output) = search.output else {
            panic!("unexpected search output");
        };
        assert!(search_output
            .matches
            .iter()
            .any(|tool_match| tool_match.tool_name == AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS));

        let access = runtime
            .tool_access(AutonomousToolAccessRequest {
                action: AutonomousToolAccessAction::Request,
                groups: vec!["system_diagnostics".into()],
                tools: Vec::new(),
                reason: Some("Inspect process open files.".into()),
            })
            .expect("tool access");
        let AutonomousToolOutput::ToolAccess(access_output) = access.output else {
            panic!("unexpected access output");
        };
        assert_eq!(
            access_output.granted_tools,
            vec![AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS.to_string()]
        );
        assert!(access_output.denied_tools.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn process_open_files_reports_current_process_temp_file_and_listener() {
        let repo = tempfile::tempdir().expect("repo");
        let runtime = AutonomousToolRuntime::new(repo.path()).expect("runtime");
        let mut temp_file = NamedTempFile::new().expect("temp file");
        writeln!(temp_file, "held open").expect("write temp file");
        temp_file.flush().expect("flush temp file");
        let temp_path = temp_file
            .path()
            .canonicalize()
            .expect("canonical temp path");
        let temp_path = temp_path.to_string_lossy().into_owned();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let port = listener.local_addr().expect("local addr").port();

        let mut last_rows = Vec::new();
        for _ in 0..10 {
            let output = system_diagnostics_output(
                runtime
                    .system_diagnostics(open_files_request(std::process::id()))
                    .expect("open files diagnostics"),
            );
            if output
                .rows
                .iter()
                .any(|row| row.path.as_deref() == Some(&temp_path))
                && output.rows.iter().any(|row| {
                    row.fd_kind == Some(AutonomousSystemDiagnosticsFdKind::Socket)
                        && row.local_port == Some(port)
                })
            {
                assert!(output.performed);
                assert!(!output.summary.contains("lsof -"));
                return;
            }
            last_rows = output.rows;
            thread::sleep(Duration::from_millis(100));
        }

        panic!(
            "process_open_files did not report temp file {temp_path} and listener {port}; rows: {last_rows:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn diagnostics_bundle_hung_process_reports_compact_sections_and_ports() {
        let repo = tempfile::tempdir().expect("repo");
        let runtime = AutonomousToolRuntime::new(repo.path()).expect("runtime");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let port = listener.local_addr().expect("local addr").port();
        let mut request = diagnostics_request(
            AutonomousSystemDiagnosticsAction::DiagnosticsBundle,
            std::process::id(),
        );
        request.preset = Some(AutonomousSystemDiagnosticsPreset::HungProcess);
        request.limit = Some(100);

        let output = system_diagnostics_output(
            runtime
                .system_diagnostics(request)
                .expect("diagnostics bundle"),
        );

        assert_eq!(
            output.action,
            AutonomousSystemDiagnosticsAction::DiagnosticsBundle
        );
        assert!(output.summary.contains("hung_process"));
        assert!(output.rows.iter().any(|row| {
            row.row_type == "diagnostics_bundle_section"
                && row.platform.get("bundle_action").map(String::as_str)
                    == Some("process_resource_snapshot")
        }));
        assert!(output.rows.iter().any(|row| {
            row.row_type == "process_resource_snapshot" && row.pid == Some(std::process::id())
        }));
        assert!(output.rows.iter().any(|row| {
            row.fd_kind == Some(AutonomousSystemDiagnosticsFdKind::Socket)
                && row.local_port == Some(port)
                && row.platform.get("bundle_action").map(String::as_str)
                    == Some("process_open_files")
        }));
        if cfg!(target_os = "macos") {
            assert!(!output.performed);
            assert!(output.policy.approval_required);
            assert!(output.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "system_diagnostics_bundle_action_blocked"
            }));
        } else {
            assert!(output.performed);
        }
    }

    #[cfg(unix)]
    #[test]
    fn process_resource_snapshot_reports_current_process() {
        let repo = tempfile::tempdir().expect("repo");
        let runtime = AutonomousToolRuntime::new(repo.path()).expect("runtime");

        let output = system_diagnostics_output(
            runtime
                .system_diagnostics(diagnostics_request(
                    AutonomousSystemDiagnosticsAction::ProcessResourceSnapshot,
                    std::process::id(),
                ))
                .expect("resource snapshot diagnostics"),
        );

        assert!(output.performed);
        assert_eq!(
            output.action,
            AutonomousSystemDiagnosticsAction::ProcessResourceSnapshot
        );
        assert!(output.rows.iter().any(|row| {
            row.row_type == "process_resource_snapshot"
                && row.pid == Some(std::process::id())
                && row.memory_bytes.is_some()
        }));
    }

    #[cfg(unix)]
    #[test]
    fn process_threads_reports_current_process_threads() {
        let repo = tempfile::tempdir().expect("repo");
        let runtime = AutonomousToolRuntime::new(repo.path()).expect("runtime");

        let output = system_diagnostics_output(
            runtime
                .system_diagnostics(diagnostics_request(
                    AutonomousSystemDiagnosticsAction::ProcessThreads,
                    std::process::id(),
                ))
                .expect("thread diagnostics"),
        );

        assert!(output.performed);
        assert_eq!(
            output.action,
            AutonomousSystemDiagnosticsAction::ProcessThreads
        );
        assert!(output
            .rows
            .iter()
            .any(|row| row.row_type == "process_thread" && row.pid == Some(std::process::id())));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn process_sample_requires_operator_approval() {
        let repo = tempfile::tempdir().expect("repo");
        let runtime = AutonomousToolRuntime::new(repo.path()).expect("runtime");

        let output = system_diagnostics_output(
            runtime
                .system_diagnostics(diagnostics_request(
                    AutonomousSystemDiagnosticsAction::ProcessSample,
                    std::process::id(),
                ))
                .expect("process sample diagnostics"),
        );

        assert!(!output.performed);
        assert_eq!(
            output.action,
            AutonomousSystemDiagnosticsAction::ProcessSample
        );
        assert!(output.policy.approval_required);
        assert!(output
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "system_diagnostics_approval_required" }));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn process_sample_writes_bounded_artifact_for_spawned_process() {
        let repo = tempfile::tempdir().expect("repo");
        let runtime = AutonomousToolRuntime::new(repo.path()).expect("runtime");
        let mut child = ProcessCommand::new("/bin/sleep")
            .arg("5")
            .spawn()
            .expect("spawn sleep");
        let pid = child.id();
        let mut request =
            diagnostics_request(AutonomousSystemDiagnosticsAction::ProcessSample, pid);
        request.duration_ms = Some(1_000);
        request.interval_ms = Some(10);
        request.max_artifact_bytes = Some(1_024);

        let output = system_diagnostics_output(
            runtime
                .system_diagnostics_with_operator_approval(request)
                .expect("approved process sample diagnostics"),
        );
        let _ = child.kill();
        let _ = child.wait();

        assert!(output.performed, "{output:?}");
        let artifact = output.artifact.expect("artifact");
        assert!(artifact.path.ends_with(".txt"));
        assert!(artifact.byte_count <= 1_024);
        assert!(std::path::Path::new(&artifact.path).exists());
        assert!(output.rows.iter().any(|row| {
            row.row_type == "process_sample"
                && row.pid == Some(pid)
                && row.platform.get("source").map(String::as_str) == Some("sample")
        }));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_accessibility_snapshot_requires_operator_approval() {
        let repo = tempfile::tempdir().expect("repo");
        let runtime = AutonomousToolRuntime::new(repo.path()).expect("runtime");
        let mut request = diagnostics_request(
            AutonomousSystemDiagnosticsAction::MacosAccessibilitySnapshot,
            std::process::id(),
        );
        request.focused_only = true;

        let output = system_diagnostics_output(
            runtime
                .system_diagnostics(request)
                .expect("accessibility diagnostics"),
        );

        assert!(!output.performed);
        assert_eq!(
            output.action,
            AutonomousSystemDiagnosticsAction::MacosAccessibilitySnapshot
        );
        assert!(output.policy.approval_required);
        assert!(output
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "system_diagnostics_approval_required" }));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn approved_macos_accessibility_snapshot_returns_permission_diagnostic_or_rows() {
        let repo = tempfile::tempdir().expect("repo");
        let runtime = AutonomousToolRuntime::new(repo.path()).expect("runtime");
        let mut request = diagnostics_request(
            AutonomousSystemDiagnosticsAction::MacosAccessibilitySnapshot,
            std::process::id(),
        );
        request.focused_only = true;
        request.limit = Some(20);

        let output = system_diagnostics_output(
            runtime
                .system_diagnostics_with_operator_approval(request)
                .expect("approved accessibility diagnostics"),
        );

        if output.performed {
            assert!(!output.rows.is_empty());
            assert!(output
                .rows
                .iter()
                .all(|row| { row.row_type.starts_with("macos_accessibility_") }));
        } else {
            assert!(output.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "system_diagnostics_accessibility_permission_denied"
            }));
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn system_log_query_for_missing_process_returns_bounded_result() {
        let repo = tempfile::tempdir().expect("repo");
        let runtime = AutonomousToolRuntime::new(repo.path()).expect("runtime");
        let mut request = diagnostics_request(AutonomousSystemDiagnosticsAction::SystemLogQuery, 1);
        request.pid = None;
        request.process_name = Some("__xero_no_such_process__".into());
        request.last_ms = Some(1_000);
        request.limit = Some(10);

        let output = system_diagnostics_output(
            runtime
                .system_diagnostics(request)
                .expect("system log diagnostics"),
        );

        assert!(output.performed);
        assert_eq!(
            output.action,
            AutonomousSystemDiagnosticsAction::SystemLogQuery
        );
        assert!(output.rows.len() <= 10);
    }

    fn open_files_request(pid: u32) -> AutonomousSystemDiagnosticsRequest {
        let mut request =
            diagnostics_request(AutonomousSystemDiagnosticsAction::ProcessOpenFiles, pid);
        request.limit = Some(500);
        request
    }

    fn diagnostics_request(
        action: AutonomousSystemDiagnosticsAction,
        pid: u32,
    ) -> AutonomousSystemDiagnosticsRequest {
        AutonomousSystemDiagnosticsRequest {
            action,
            preset: None,
            pid: Some(pid),
            process_name: None,
            bundle_id: None,
            app_name: None,
            window_id: None,
            since: None,
            duration_ms: None,
            interval_ms: None,
            limit: Some(100),
            filter: None,
            include_children: false,
            artifact_mode: None,
            fd_kinds: Vec::new(),
            include_sockets: false,
            include_files: false,
            include_deleted: false,
            sample_count: None,
            include_ports: false,
            include_threads_summary: false,
            include_wait_channel: false,
            include_stack_hints: false,
            max_artifact_bytes: None,
            last_ms: None,
            level: None,
            subsystem: None,
            category: None,
            message_contains: None,
            process_predicate: None,
            max_depth: None,
            focused_only: false,
            attributes: Vec::new(),
        }
    }

    fn system_diagnostics_output(
        result: AutonomousToolResult,
    ) -> AutonomousSystemDiagnosticsOutput {
        match result.output {
            AutonomousToolOutput::SystemDiagnostics(output) => output,
            other => panic!("expected system diagnostics output, got {other:?}"),
        }
    }

    fn test_engineer_runtime(repo_root: &std::path::Path) -> AutonomousToolRuntime {
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
}
