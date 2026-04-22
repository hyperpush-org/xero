use std::{
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use super::{
    policy::CommandPolicyDecision, repo_scope::display_relative_or_root, AutonomousCommandOutput,
    AutonomousToolCommandResult, AutonomousToolOutput, AutonomousToolResult, AutonomousToolRuntime,
    AUTONOMOUS_TOOL_COMMAND,
};

use crate::commands::{CommandError, CommandResult};

const REDACTED_COMMAND_OUTPUT_SUMMARY: &str =
    "Command output was redacted before durable persistence.";

impl AutonomousToolRuntime {
    pub fn command(
        &self,
        request: super::AutonomousCommandRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let decision = self.evaluate_command_policy(self.prepare_command_request(request)?)?;

        match decision {
            CommandPolicyDecision::Allow { prepared, policy } => {
                let mut command = Command::new(&prepared.argv[0]);
                command
                    .args(prepared.argv.iter().skip(1))
                    .current_dir(&prepared.cwd)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());

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

                let (status, timed_out) = loop {
                    match child.try_wait() {
                        Ok(Some(status)) => break (status, false),
                        Ok(None) if started_at.elapsed() >= timeout_duration => {
                            let _ = child.kill();
                            let status = child.wait().map_err(|error| {
                                CommandError::system_fault(
                                    "autonomous_tool_command_wait_failed",
                                    format!(
                                        "Cadence could not stop timed-out command `{}`: {error}",
                                        prepared.argv[0]
                                    ),
                                )
                            })?;
                            break (status, true);
                        }
                        Ok(None) => thread::sleep(Duration::from_millis(10)),
                        Err(error) => {
                            let _ = child.kill();
                            let _ = child.wait();
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
                        argv: prepared.argv,
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
            CommandPolicyDecision::Escalate { prepared, policy } => {
                let cwd = prepared
                    .cwd_relative
                    .as_ref()
                    .map(|path| {
                        display_relative_or_root(&self.repo_root, &self.repo_root.join(path))
                    })
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
                        argv: prepared.argv,
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
    }
}

fn render_command_for_summary(argv: &[String]) -> String {
    argv.join(" ")
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

fn find_prohibited_persistence_content(value: &str) -> Option<&'static str> {
    let normalized = value.to_ascii_lowercase();

    if normalized.contains("access_token")
        || normalized.contains("refresh_token")
        || normalized.contains("bearer ")
        || normalized.contains("oauth")
        || normalized.contains("sk-")
    {
        return Some("OAuth or API token material");
    }

    if normalized.contains("transcript") {
        return Some("runtime transcript text");
    }

    if normalized.contains("tool_payload")
        || normalized.contains("tool payload")
        || normalized.contains("raw payload")
    {
        return Some("tool raw payload data");
    }

    if normalized.contains("redirect_uri")
        || normalized.contains("authorization_url")
        || normalized.contains("/auth/callback")
        || normalized.contains("127.0.0.1:")
        || normalized.contains("localhost:")
    {
        return Some("OAuth redirect URL data");
    }

    if normalized.contains("chatgpt_account_id")
        || (normalized.contains("session_id") && normalized.contains("provider_id"))
    {
        return Some("auth-store contents");
    }

    if value.contains('\u{1b}')
        || value.contains('\0')
        || value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Some("raw PTY byte sequences");
    }

    None
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
