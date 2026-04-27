use std::path::{Path, PathBuf};

use super::{
    repo_scope::normalize_relative_path, AutonomousCommandPolicyOutcome,
    AutonomousCommandPolicyTrace, AutonomousCommandRequest, AutonomousProcessActionRiskLevel,
    AutonomousProcessManagerAction, AutonomousProcessManagerPolicyTrace,
    AutonomousProcessOwnershipScope, AutonomousToolRuntime, DEFAULT_COMMAND_TIMEOUT_MS,
};
use crate::commands::{
    validate_non_empty, CommandError, CommandErrorClass, CommandResult, RuntimeRunApprovalModeDto,
};
use crate::runtime::redaction::render_command_for_persistence;

#[derive(Debug, Clone)]
pub(super) struct PreparedCommandRequest {
    pub(super) argv: Vec<String>,
    pub(super) cwd_relative: Option<PathBuf>,
    pub(super) cwd: PathBuf,
    pub(super) timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub(super) enum CommandPolicyDecision {
    Allow {
        prepared: PreparedCommandRequest,
        policy: AutonomousCommandPolicyTrace,
    },
    Escalate {
        prepared: PreparedCommandRequest,
        policy: AutonomousCommandPolicyTrace,
    },
}

impl AutonomousToolRuntime {
    pub(super) fn prepare_command_request(
        &self,
        request: AutonomousCommandRequest,
    ) -> CommandResult<PreparedCommandRequest> {
        let argv = normalize_command_argv(&request.argv)?;
        let cwd_relative = request
            .cwd
            .as_deref()
            .map(normalize_command_cwd)
            .transpose()?;
        let cwd = match cwd_relative.as_ref() {
            Some(path) => self
                .resolve_existing_directory(path)
                .map_err(map_cwd_policy_error)?,
            None => self.repo_root.clone(),
        };
        let timeout_ms =
            normalize_timeout_ms(request.timeout_ms, self.limits.max_command_timeout_ms)?;

        Ok(PreparedCommandRequest {
            argv,
            cwd_relative,
            cwd,
            timeout_ms,
        })
    }

    pub(super) fn evaluate_command_policy(
        &self,
        prepared: PreparedCommandRequest,
    ) -> CommandResult<CommandPolicyDecision> {
        let control_state = self.command_controls.as_ref().ok_or_else(|| {
            CommandError::new(
                "policy_denied_approval_snapshot_missing",
                CommandErrorClass::PolicyDenied,
                "Cadence denied the autonomous shell command because no active approval snapshot was available.",
                false,
            )
        })?;
        let active = &control_state.active;
        if active.model_id.trim().is_empty() || active.applied_at.trim().is_empty() {
            return Err(CommandError::new(
                "policy_denied_approval_snapshot_invalid",
                CommandErrorClass::PolicyDenied,
                "Cadence denied the autonomous shell command because the active approval snapshot was malformed.",
                false,
            ));
        }

        validate_repo_scoped_arguments(&prepared, active.approval_mode.clone())?;

        if active.approval_mode != RuntimeRunApprovalModeDto::Yolo {
            let policy = policy_trace(
                AutonomousCommandPolicyOutcome::Escalated,
                active.approval_mode.clone(),
                "policy_escalated_approval_mode",
                format!(
                    "Active approval mode `{}` requires operator review before autonomous shell commands can run.",
                    approval_mode_label(&active.approval_mode)
                ),
            );
            return Ok(CommandPolicyDecision::Escalate { prepared, policy });
        }

        let policy = match classify_command(&prepared.argv) {
            CommandClassification::Safe(reason) => policy_trace(
                AutonomousCommandPolicyOutcome::Allowed,
                active.approval_mode.clone(),
                "policy_allowed_repo_scoped_command",
                reason,
            ),
            CommandClassification::Destructive { code, reason }
            | CommandClassification::Ambiguous { code, reason } => {
                return Ok(CommandPolicyDecision::Escalate {
                    prepared,
                    policy: policy_trace(
                        AutonomousCommandPolicyOutcome::Escalated,
                        active.approval_mode.clone(),
                        code,
                        reason,
                    ),
                });
            }
        };

        Ok(CommandPolicyDecision::Allow { prepared, policy })
    }
}

pub(super) fn process_manager_policy_trace(
    action: AutonomousProcessManagerAction,
    target_ownership: Option<AutonomousProcessOwnershipScope>,
    persistent: bool,
) -> AutonomousProcessManagerPolicyTrace {
    let target_scope = target_ownership.unwrap_or(AutonomousProcessOwnershipScope::CadenceOwned);
    let risk_level = match action {
        AutonomousProcessManagerAction::List
        | AutonomousProcessManagerAction::Status
        | AutonomousProcessManagerAction::Output
        | AutonomousProcessManagerAction::Digest
        | AutonomousProcessManagerAction::WaitForReady
        | AutonomousProcessManagerAction::Highlights
        | AutonomousProcessManagerAction::Env
        | AutonomousProcessManagerAction::GroupStatus
        | AutonomousProcessManagerAction::AsyncAwait => AutonomousProcessActionRiskLevel::Observe,
        AutonomousProcessManagerAction::Start if persistent => {
            AutonomousProcessActionRiskLevel::PersistentBackground
        }
        AutonomousProcessManagerAction::Start
        | AutonomousProcessManagerAction::AsyncStart
        | AutonomousProcessManagerAction::Send
        | AutonomousProcessManagerAction::SendAndWait
        | AutonomousProcessManagerAction::Run => AutonomousProcessActionRiskLevel::RunOwned,
        AutonomousProcessManagerAction::Signal
        | AutonomousProcessManagerAction::Kill
        | AutonomousProcessManagerAction::GroupKill
        | AutonomousProcessManagerAction::AsyncCancel
        | AutonomousProcessManagerAction::Restart
            if target_scope == AutonomousProcessOwnershipScope::External =>
        {
            AutonomousProcessActionRiskLevel::SignalExternal
        }
        AutonomousProcessManagerAction::Signal
        | AutonomousProcessManagerAction::Kill
        | AutonomousProcessManagerAction::GroupKill
        | AutonomousProcessManagerAction::AsyncCancel
        | AutonomousProcessManagerAction::Restart => AutonomousProcessActionRiskLevel::SignalOwned,
    };

    let (approval_required, code, reason) = match risk_level {
        AutonomousProcessActionRiskLevel::Observe => (
            false,
            "process_policy_observe",
            "Observing process metadata or bounded output is read-only and does not require operator approval.",
        ),
        AutonomousProcessActionRiskLevel::RunOwned => (
            true,
            "process_policy_run_owned_requires_command_policy",
            "Starting or interacting with Cadence-owned processes must pass the existing repo-scoped command approval policy before implementation.",
        ),
        AutonomousProcessActionRiskLevel::SignalOwned => (
            false,
            "process_policy_signal_owned",
            "Signaling Cadence-owned processes is allowed after ownership verification and audit logging.",
        ),
        AutonomousProcessActionRiskLevel::SignalExternal => (
            true,
            "process_policy_signal_external_requires_approval",
            "Signaling external processes requires explicit operator approval.",
        ),
        AutonomousProcessActionRiskLevel::PersistentBackground => (
            true,
            "process_policy_persistent_background_requires_approval",
            "Persistent background processes require explicit operator approval and durable lifecycle metadata.",
        ),
        AutonomousProcessActionRiskLevel::SystemRead => (
            true,
            "process_policy_system_read_requires_approval",
            "System reads outside the repository require explicit operator approval.",
        ),
        AutonomousProcessActionRiskLevel::OsAutomation => (
            true,
            "process_policy_os_automation_requires_approval",
            "Operating-system automation requires explicit operator approval.",
        ),
    };

    AutonomousProcessManagerPolicyTrace {
        risk_level,
        approval_required,
        code: code.into(),
        reason: reason.into(),
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

fn normalize_command_cwd(value: &str) -> CommandResult<PathBuf> {
    validate_non_empty(value, "cwd")?;
    normalize_relative_path(value, "cwd").map_err(map_cwd_policy_error)
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

fn validate_repo_scoped_arguments(
    prepared: &PreparedCommandRequest,
    approval_mode: RuntimeRunApprovalModeDto,
) -> CommandResult<()> {
    for argument in prepared.argv.iter().skip(1) {
        let Some(candidate) = extract_path_candidate(argument) else {
            continue;
        };

        normalize_relative_path(candidate, "argv").map_err(|error| {
            if error.class == CommandErrorClass::PolicyDenied {
                CommandError::new(
                    "policy_denied_argument_outside_repo",
                    CommandErrorClass::PolicyDenied,
                    format!(
                        "Cadence denied the autonomous shell command under active approval mode `{}` because argument `{candidate}` escapes the imported repository root.",
                        approval_mode_label(&approval_mode)
                    ),
                    false,
                )
            } else {
                error
            }
        })?;
    }

    Ok(())
}

fn extract_path_candidate(argument: &str) -> Option<&str> {
    if argument == "--" {
        return None;
    }

    if argument.starts_with('-') {
        if let Some((_, value)) = argument.split_once('=') {
            if looks_like_path(value) {
                return Some(value);
            }
        }
        return None;
    }

    if looks_like_path(argument) {
        return Some(argument);
    }

    None
}

fn looks_like_path(argument: &str) -> bool {
    argument == "."
        || argument == ".."
        || argument.starts_with("./")
        || argument.starts_with("../")
        || argument.contains('/')
        || argument.contains('\\')
        || Path::new(argument).is_absolute()
}

fn map_cwd_policy_error(error: CommandError) -> CommandError {
    if error.class == CommandErrorClass::PolicyDenied {
        return CommandError::new(
            "policy_denied_command_cwd_outside_repo",
            CommandErrorClass::PolicyDenied,
            "Cadence denied the autonomous shell command because its cwd escapes the imported repository root.",
            false,
        );
    }

    error
}

#[derive(Debug, Clone)]
enum CommandClassification {
    Safe(String),
    Destructive { code: &'static str, reason: String },
    Ambiguous { code: &'static str, reason: String },
}

fn classify_command(argv: &[String]) -> CommandClassification {
    let program = executable_name(&argv[0]);

    if is_shell_wrapper(program) {
        if shell_wrapper_contains_sensitive_pattern(argv) {
            return CommandClassification::Ambiguous {
                code: "policy_escalated_sensitive_shell",
                reason: format!(
                    "Cadence requires operator review for shell wrapper command `{}` because the script may expand environment variables, access absolute paths, or contact external network surfaces.",
                    render_command_for_summary(argv)
                ),
            };
        }
        if shell_wrapper_contains_destructive_pattern(argv) {
            return CommandClassification::Destructive {
                code: "policy_escalated_destructive_shell",
                reason: format!(
                    "Cadence requires operator review for shell wrapper command `{}` because the script text matches the destructive command classifier.",
                    render_command_for_summary(argv)
                ),
            };
        }
        return CommandClassification::Safe(format!(
            "Active approval mode `yolo` allowed repo-scoped shell wrapper command `{}` because no destructive shell pattern was detected.",
            render_command_for_summary(argv)
        ));
    }

    match program {
        "curl" | "wget" | "nc" | "netcat" | "ssh" | "scp" | "sftp" | "ftp" | "ping" => {
            CommandClassification::Ambiguous {
                code: "policy_escalated_network_command",
                reason: format!(
                    "Cadence requires operator review for `{}` because it can contact external network surfaces.",
                    render_command_for_summary(argv)
                ),
            }
        }
        "openssl" if argv.iter().any(|argument| argument == "s_client") => {
            CommandClassification::Ambiguous {
                code: "policy_escalated_network_command",
                reason: format!(
                    "Cadence requires operator review for `{}` because it can contact external network surfaces.",
                    render_command_for_summary(argv)
                ),
            }
        }
        "pwd" | "ls" | "dir" | "echo" | "cat" | "type" | "head" | "tail" | "grep"
        | "rg" | "sleep" => CommandClassification::Safe(format!(
            "Active approval mode `yolo` allowed repo-scoped command `{}` because it matched the non-destructive command classifier.",
            render_command_for_summary(argv)
        )),
        "find" => {
            if argv.iter().any(|argument| argument == "-delete") {
                return CommandClassification::Destructive {
                    code: "policy_escalated_destructive_command",
                    reason: format!(
                        "Cadence requires operator review for `{}` because `find -delete` is destructive.",
                        render_command_for_summary(argv)
                    ),
                };
            }
            CommandClassification::Safe(format!(
                "Active approval mode `yolo` allowed repo-scoped command `{}` because it matched the non-destructive command classifier.",
                render_command_for_summary(argv)
            ))
        }
        "git" => classify_git_command(argv),
        "cargo" => classify_cargo_command(argv),
        "npm" | "pnpm" | "yarn" | "bun" => classify_package_manager_command(argv),
        "rm" | "rmdir" | "del" | "erase" | "rd" | "mv" | "move" | "chmod" | "chown"
        | "dd" | "mkfs" | "diskutil" => CommandClassification::Destructive {
            code: "policy_escalated_destructive_command",
            reason: format!(
                "Cadence requires operator review for `{}` because it matches the destructive command classifier.",
                render_command_for_summary(argv)
            ),
        },
        _ => CommandClassification::Ambiguous {
            code: "policy_escalated_ambiguous_command",
            reason: format!(
                "Cadence could not classify `{}` as a repo-scoped non-destructive command, so operator review is required.",
                render_command_for_summary(argv)
            ),
        },
    }
}

fn classify_git_command(argv: &[String]) -> CommandClassification {
    let subcommand = argv
        .iter()
        .skip(1)
        .find(|argument| !argument.starts_with('-'));
    match subcommand.map(String::as_str) {
        Some("status" | "diff" | "log" | "show" | "rev-parse" | "grep" | "ls-files") => {
            safe_command(argv)
        }
        Some("branch") => {
            if argv.iter().any(|argument| matches!(argument.as_str(), "-d" | "-D" | "--delete")) {
                destructive_command(argv, "git branch delete flags are destructive")
            } else {
                safe_command(argv)
            }
        }
        Some("tag") => {
            if argv.iter().any(|argument| matches!(argument.as_str(), "-d" | "--delete")) {
                destructive_command(argv, "git tag delete flags are destructive")
            } else {
                safe_command(argv)
            }
        }
        Some(
            "add"
            | "commit"
            | "mv"
            | "rm",
        ) => safe_command(argv),
        Some(
            "clean"
            | "reset"
            | "checkout"
            | "switch"
            | "restore"
            | "stash"
            | "merge"
            | "rebase"
            | "cherry-pick"
            | "revert"
            | "push"
            | "pull",
        ) => destructive_command(argv, "the git subcommand mutates repository state"),
        Some(_) | None => CommandClassification::Ambiguous {
            code: "policy_escalated_ambiguous_command",
            reason: format!(
                "Cadence could not classify git command `{}` as non-destructive, so operator review is required.",
                render_command_for_summary(argv)
            ),
        },
    }
}

fn classify_cargo_command(argv: &[String]) -> CommandClassification {
    let subcommand = argv
        .iter()
        .skip(1)
        .find(|argument| !argument.starts_with('-'));
    match subcommand.map(String::as_str) {
        Some("check" | "clippy" | "doc" | "metadata" | "test" | "tree" | "build" | "fmt") => {
            safe_command(argv)
        }
        Some(_) | None => CommandClassification::Ambiguous {
            code: "policy_escalated_ambiguous_command",
            reason: format!(
                "Cadence could not classify cargo command `{}` as non-destructive, so operator review is required.",
                render_command_for_summary(argv)
            ),
        },
    }
}

fn classify_package_manager_command(argv: &[String]) -> CommandClassification {
    let subcommand = argv
        .iter()
        .skip(1)
        .find(|argument| !argument.starts_with('-'));
    match subcommand.map(String::as_str) {
        Some("install" | "add" | "remove" | "unlink" | "upgrade" | "update") => {
            CommandClassification::Ambiguous {
                code: "policy_escalated_package_manager_mutation",
                reason: format!(
                    "Cadence requires operator review for `{}` because package-manager mutation commands can execute install scripts, change dependency state, or contact external registries.",
                    render_command_for_summary(argv)
                ),
            }
        }
        Some("test" | "lint" | "typecheck" | "build") => safe_command(argv),
        Some("exec") => CommandClassification::Ambiguous {
            code: "policy_escalated_package_manager_exec",
            reason: format!(
                "Cadence requires operator review for `{}` because package-manager exec commands can run arbitrary local or registry-provided binaries.",
                render_command_for_summary(argv)
            ),
        },
        Some("publish") => destructive_command(argv, "package manager publish commands can affect external registries"),
        Some("run") => {
            CommandClassification::Ambiguous {
                code: "policy_escalated_package_manager_run",
                reason: format!(
                    "Cadence requires operator review for `{}` because package-manager scripts are project-defined commands and can perform arbitrary side effects.",
                    render_command_for_summary(argv)
                ),
            }
        }
        Some(_) | None => CommandClassification::Ambiguous {
            code: "policy_escalated_ambiguous_command",
            reason: format!(
                "Cadence could not classify package-manager command `{}` as non-destructive, so operator review is required.",
                render_command_for_summary(argv)
            ),
        },
    }
}

fn safe_command(argv: &[String]) -> CommandClassification {
    CommandClassification::Safe(format!(
        "Active approval mode `yolo` allowed repo-scoped command `{}` because it matched the non-destructive command classifier.",
        render_command_for_summary(argv)
    ))
}

fn destructive_command(argv: &[String], reason: &str) -> CommandClassification {
    CommandClassification::Destructive {
        code: "policy_escalated_destructive_command",
        reason: format!(
            "Cadence requires operator review for `{}` because {reason}.",
            render_command_for_summary(argv)
        ),
    }
}

fn executable_name(program: &str) -> &str {
    Path::new(program)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(program)
        .trim()
}

fn is_shell_wrapper(program: &str) -> bool {
    matches!(
        program.to_ascii_lowercase().as_str(),
        "sh" | "bash"
            | "zsh"
            | "fish"
            | "dash"
            | "ksh"
            | "cmd"
            | "cmd.exe"
            | "powershell"
            | "powershell.exe"
            | "pwsh"
            | "pwsh.exe"
    )
}

fn shell_wrapper_contains_sensitive_pattern(argv: &[String]) -> bool {
    let program = executable_name(&argv[0]).to_ascii_lowercase();
    let windows_shell = matches!(program.as_str(), "cmd" | "cmd.exe");
    let normalized = argv
        .iter()
        .skip(1)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    if normalized.contains('$')
        || normalized.contains('`')
        || normalized.contains("://")
        || normalized.contains(">/")
        || normalized.contains("</")
        || normalized.contains("../")
        || normalized.contains("~/")
    {
        return true;
    }

    if windows_shell {
        if normalized.contains('%')
            || normalized.contains(":\\")
            || normalized.contains("..\\")
            || normalized.contains("~\\")
        {
            return true;
        }
    } else if normalized.contains(" /") || normalized.starts_with('/') || normalized.contains("\t/")
    {
        return true;
    }

    [
        "curl ",
        " wget ",
        "wget ",
        "nc ",
        " netcat ",
        "ssh ",
        " scp ",
        "sftp ",
        "ftp ",
        "openssl s_client",
        "/dev/tcp/",
        "/dev/udp/",
    ]
    .iter()
    .any(|pattern| normalized.contains(pattern))
}

fn shell_wrapper_contains_destructive_pattern(argv: &[String]) -> bool {
    let normalized = argv.join(" ").to_ascii_lowercase();
    [
        "rm ",
        " rm ",
        " rm-",
        "del ",
        " del ",
        " erase ",
        "rmdir ",
        " rmdir ",
        " rd ",
        "chmod ",
        " chmod ",
        "chown ",
        " chown ",
        " git clean",
        " git reset",
        " git checkout",
        " git switch",
        " git restore",
        " git stash",
        " mkfs",
        " diskutil",
        " sudo ",
    ]
    .iter()
    .any(|pattern| normalized.contains(pattern))
}

fn render_command_for_summary(argv: &[String]) -> String {
    render_command_for_persistence(argv)
}

fn approval_mode_label(mode: &RuntimeRunApprovalModeDto) -> &'static str {
    match mode {
        RuntimeRunApprovalModeDto::Suggest => "suggest",
        RuntimeRunApprovalModeDto::AutoEdit => "auto_edit",
        RuntimeRunApprovalModeDto::Yolo => "yolo",
    }
}

fn policy_trace(
    outcome: AutonomousCommandPolicyOutcome,
    approval_mode: RuntimeRunApprovalModeDto,
    code: impl Into<String>,
    reason: impl Into<String>,
) -> AutonomousCommandPolicyTrace {
    AutonomousCommandPolicyTrace {
        outcome,
        approval_mode,
        code: code.into(),
        reason: reason.into(),
    }
}
