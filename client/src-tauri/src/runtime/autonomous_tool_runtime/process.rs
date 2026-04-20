use std::{
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use super::{
    repo_scope::{display_relative_or_root, normalize_relative_path},
    AutonomousCommandOutput, AutonomousCommandRequest, AutonomousToolCommandResult,
    AutonomousToolOutput, AutonomousToolResult, AutonomousToolRuntime, AUTONOMOUS_TOOL_COMMAND,
    DEFAULT_COMMAND_TIMEOUT_MS,
};

use crate::commands::{validate_non_empty, CommandError, CommandResult};

const REDACTED_COMMAND_OUTPUT_SUMMARY: &str =
    "Command output was redacted before durable persistence.";

impl AutonomousToolRuntime {
    pub fn command(
        &self,
        request: AutonomousCommandRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let argv = normalize_command_argv(&request.argv)?;
        let cwd_relative = request
            .cwd
            .as_deref()
            .map(|value| {
                validate_non_empty(value, "cwd")?;
                normalize_relative_path(value, "cwd")
            })
            .transpose()?;
        let cwd = match cwd_relative.as_ref() {
            Some(path) => self.resolve_existing_directory(path)?,
            None => self.repo_root.clone(),
        };
        let timeout = normalize_timeout_ms(request.timeout_ms, self.limits.max_command_timeout_ms)?;

        let mut command = Command::new(&argv[0]);
        command
            .args(argv.iter().skip(1))
            .current_dir(&cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => CommandError::user_fixable(
                "autonomous_tool_command_not_found",
                format!("Cadence could not find command `{}`.", argv[0]),
            ),
            _ => CommandError::system_fault(
                "autonomous_tool_command_spawn_failed",
                format!("Cadence could not launch command `{}`: {error}", argv[0]),
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
        let timeout_duration = Duration::from_millis(timeout);

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
                                argv[0]
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
                            argv[0]
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
                    "Cadence timed out command `{}` after {timeout}ms.",
                    render_command_for_summary(&argv)
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
            summary: command_result_summary(&argv, exit_code),
        };
        let summary = match exit_code {
            Some(0) => format!(
                "Command `{}` exited successfully in `{}`.",
                render_command_for_summary(&argv),
                display_relative_or_root(&self.repo_root, &cwd)
            ),
            Some(code) => format!(
                "Command `{}` exited with code {code} in `{}`.",
                render_command_for_summary(&argv),
                display_relative_or_root(&self.repo_root, &cwd)
            ),
            None => format!(
                "Command `{}` terminated without an exit code in `{}`.",
                render_command_for_summary(&argv),
                display_relative_or_root(&self.repo_root, &cwd)
            ),
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_COMMAND.into(),
            summary,
            command_result: Some(command_result.clone()),
            output: AutonomousToolOutput::Command(AutonomousCommandOutput {
                argv,
                cwd: display_relative_or_root(&self.repo_root, &cwd),
                stdout: stdout_excerpt.text,
                stderr: stderr_excerpt.text,
                stdout_truncated: stdout_excerpt.truncated,
                stderr_truncated: stderr_excerpt.truncated,
                stdout_redacted: stdout_excerpt.redacted,
                stderr_redacted: stderr_excerpt.redacted,
                exit_code,
                timed_out: false,
            }),
        })
    }
}

fn normalize_command_argv(argv: &[String]) -> CommandResult<Vec<String>> {
    if argv.is_empty() || argv[0].trim().is_empty() {
        return Err(CommandError::user_fixable(
            "autonomous_tool_command_invalid",
            "Cadence requires autonomous command requests to include a non-empty argv[0].",
        ));
    }

    if argv.iter().any(|argument| argument.contains('\0')) {
        return Err(CommandError::user_fixable(
            "autonomous_tool_command_invalid",
            "Cadence refused a command that contained a NUL byte.",
        ));
    }

    Ok(argv
        .iter()
        .map(|argument| argument.trim().to_string())
        .collect())
}

fn normalize_timeout_ms(timeout_ms: Option<u64>, max_timeout_ms: u64) -> CommandResult<u64> {
    let timeout = timeout_ms.unwrap_or(DEFAULT_COMMAND_TIMEOUT_MS);
    if timeout == 0 || timeout > max_timeout_ms {
        return Err(CommandError::user_fixable(
            "autonomous_tool_command_timeout_invalid",
            format!("Cadence requires command timeout_ms to be between 1 and {max_timeout_ms}."),
        ));
    }
    Ok(timeout)
}

fn render_command_for_summary(argv: &[String]) -> String {
    argv.join(" ")
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
