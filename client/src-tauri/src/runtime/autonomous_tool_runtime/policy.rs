use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{
    repo_scope::normalize_relative_path, AutonomousCommandPolicyOutcome,
    AutonomousCommandPolicyTrace, AutonomousCommandRequest, AutonomousProcessActionRiskLevel,
    AutonomousProcessManagerAction, AutonomousProcessManagerPolicyTrace,
    AutonomousProcessOwnershipScope, AutonomousSafetyApprovalGrant, AutonomousSafetyPolicyAction,
    AutonomousSafetyPolicyDecision, AutonomousToolRequest, AutonomousToolRuntime,
    DEFAULT_COMMAND_TIMEOUT_MS,
};
use crate::commands::{
    validate_non_empty, CommandError, CommandErrorClass, CommandResult, RuntimeRunApprovalModeDto,
};
use crate::runtime::redaction::{
    find_prohibited_persistence_content, is_sensitive_argument_name, render_command_for_persistence,
};
use serde_json::Value as JsonValue;

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
    pub fn evaluate_safety_policy(
        &self,
        tool_name: &str,
        raw_input: &JsonValue,
        request: &AutonomousToolRequest,
        operator_approved: bool,
        input_sha256: &str,
    ) -> CommandResult<AutonomousSafetyPolicyDecision> {
        let metadata = safety_policy_metadata(request);
        let approval_mode = self
            .command_controls
            .as_ref()
            .map(|controls| controls.active.approval_mode.clone());
        let context = SafetyDecisionContext {
            tool_name,
            metadata,
            approval_mode,
            operator_approved,
            input_sha256,
        };

        if secret_like_tool_input(raw_input) {
            return Ok(safety_decision(
                AutonomousSafetyPolicyAction::Deny,
                "policy_denied_secret_like_tool_input",
                "Xero denied the tool call because its arguments contain credential-like material. Secret-bearing tool inputs are not persisted, replayed, or sent back to the model.",
                &context,
            ));
        }

        if let Some(path) = repo_path_escape(request) {
            return Ok(safety_decision(
                AutonomousSafetyPolicyAction::Deny,
                "policy_denied_path_escape",
                format!(
                    "Xero denied the tool call because path `{path}` escapes the imported repository root."
                ),
                &context,
            ));
        }

        if is_destructive_system_operation(request) {
            return Ok(safety_decision(
                AutonomousSafetyPolicyAction::Deny,
                "policy_denied_destructive_system_operation",
                "Xero denied the tool call because it targets destructive system-level operations that are not allowed in autonomous runs.",
                &context,
            ));
        }

        let command_decision = command_family_policy_decision(self, request)?;
        if let Some((action, code, explanation)) = command_decision {
            return Ok(safety_decision(action, code, explanation, &context));
        }

        if context.metadata.requires_approval && !context.operator_approved {
            return Ok(safety_decision(
                AutonomousSafetyPolicyAction::RequireApproval,
                context.metadata.require_approval_code,
                context.metadata.require_approval_reason,
                &context,
            ));
        }

        Ok(safety_decision(
            AutonomousSafetyPolicyAction::Allow,
            "policy_allowed_tool_call",
            "Xero allowed the tool call after central safety policy evaluation.",
            &context,
        ))
    }

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
                "Xero denied the autonomous shell command because no active approval snapshot was available.",
                false,
            )
        })?;
        let active = &control_state.active;
        if active.model_id.trim().is_empty() || active.applied_at.trim().is_empty() {
            return Err(CommandError::new(
                "policy_denied_approval_snapshot_invalid",
                CommandErrorClass::PolicyDenied,
                "Xero denied the autonomous shell command because the active approval snapshot was malformed.",
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

        let policy = match classify_command(&prepared) {
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

#[derive(Debug, Clone, Copy)]
struct SafetyPolicyMetadata {
    risk_class: &'static str,
    network_intent: &'static str,
    credential_sensitivity: &'static str,
    os_target: Option<&'static str>,
    prior_observation_required: bool,
    requires_approval: bool,
    require_approval_code: &'static str,
    require_approval_reason: &'static str,
}

fn safety_policy_metadata(request: &AutonomousToolRequest) -> SafetyPolicyMetadata {
    match request {
        AutonomousToolRequest::Read(request) if request.system_path => SafetyPolicyMetadata {
            risk_class: "system_read",
            network_intent: "none",
            credential_sensitivity: "possible",
            os_target: Some("filesystem"),
            prior_observation_required: false,
            requires_approval: true,
            require_approval_code: "policy_requires_approval_system_read",
            require_approval_reason: "Reading an absolute system path outside the imported repository requires operator approval.",
        },
        AutonomousToolRequest::WebSearch(_) | AutonomousToolRequest::WebFetch(_) => {
            SafetyPolicyMetadata {
                risk_class: "network",
                network_intent: "external_read",
                credential_sensitivity: "low",
                os_target: None,
                prior_observation_required: false,
                requires_approval: false,
                require_approval_code: "policy_requires_approval_network",
                require_approval_reason: "External network reads require operator approval.",
            }
        }
        AutonomousToolRequest::Browser(_) => SafetyPolicyMetadata {
            risk_class: "browser_control",
            network_intent: "browser",
            credential_sensitivity: "possible",
            os_target: Some("browser"),
            prior_observation_required: false,
            requires_approval: false,
            require_approval_code: "policy_requires_approval_browser_control",
            require_approval_reason: "Browser control requires operator approval.",
        },
        AutonomousToolRequest::MacosAutomation(_) => SafetyPolicyMetadata {
            risk_class: "os_control",
            network_intent: "none",
            credential_sensitivity: "possible",
            os_target: Some("macos"),
            prior_observation_required: false,
            requires_approval: true,
            require_approval_code: "policy_requires_approval_os_automation",
            require_approval_reason: "Operating-system automation requires operator approval.",
        },
        AutonomousToolRequest::Command(_)
        | AutonomousToolRequest::CommandSessionStart(_)
        | AutonomousToolRequest::PowerShell(_) => SafetyPolicyMetadata {
            risk_class: "command",
            network_intent: "command_dependent",
            credential_sensitivity: "possible",
            os_target: Some("process"),
            prior_observation_required: false,
            requires_approval: false,
            require_approval_code: "policy_requires_approval_command",
            require_approval_reason: "The command policy requires operator approval.",
        },
        AutonomousToolRequest::ProcessManager(request) => {
            let trace = process_manager_policy_trace(
                request.action,
                request.target_ownership,
                request.persistent,
            );
            SafetyPolicyMetadata {
                risk_class: "process_control",
                network_intent: "process_dependent",
                credential_sensitivity: "possible",
                os_target: Some("process"),
                prior_observation_required: false,
                requires_approval: trace.approval_required,
                require_approval_code: "policy_requires_approval_process_control",
                require_approval_reason: "Process control requires operator approval for this action.",
            }
        }
        AutonomousToolRequest::Edit(_)
        | AutonomousToolRequest::Write(_)
        | AutonomousToolRequest::Patch(_)
        | AutonomousToolRequest::Delete(_)
        | AutonomousToolRequest::Rename(_)
        | AutonomousToolRequest::Mkdir(_)
        | AutonomousToolRequest::NotebookEdit(_) => SafetyPolicyMetadata {
            risk_class: "write",
            network_intent: "none",
            credential_sensitivity: "possible",
            os_target: Some("filesystem"),
            prior_observation_required: true,
            requires_approval: false,
            require_approval_code: "policy_requires_approval_write",
            require_approval_reason: "Repository writes require operator approval.",
        },
        AutonomousToolRequest::Mcp(_) | AutonomousToolRequest::Skill(_) => SafetyPolicyMetadata {
            risk_class: "external_capability",
            network_intent: "external_capability_dependent",
            credential_sensitivity: "possible",
            os_target: None,
            prior_observation_required: false,
            requires_approval: false,
            require_approval_code: "policy_requires_approval_external_capability",
            require_approval_reason: "External capability invocation requires operator approval.",
        },
        AutonomousToolRequest::AgentDefinition(request) => {
            let requires_approval = matches!(
                request.action,
                super::AutonomousAgentDefinitionAction::Save
                    | super::AutonomousAgentDefinitionAction::Update
                    | super::AutonomousAgentDefinitionAction::Archive
                    | super::AutonomousAgentDefinitionAction::Clone
            );
            SafetyPolicyMetadata {
                risk_class: "agent_definition_state",
                network_intent: "none",
                credential_sensitivity: "possible",
                os_target: None,
                prior_observation_required: false,
                requires_approval,
                require_approval_code: "policy_requires_approval_agent_definition_mutation",
                require_approval_reason: "Saving, updating, archiving, or cloning agent definitions requires explicit operator approval.",
            }
        }
        AutonomousToolRequest::SolanaDeploy(_)
        | AutonomousToolRequest::SolanaCodama(_)
        | AutonomousToolRequest::SolanaReplay(_)
        | AutonomousToolRequest::SolanaSimulate(_)
        | AutonomousToolRequest::SolanaAuditExternal(_) => SafetyPolicyMetadata {
            risk_class: "external_chain_mutation",
            network_intent: "external_chain",
            credential_sensitivity: "possible",
            os_target: None,
            prior_observation_required: false,
            requires_approval: false,
            require_approval_code: "policy_requires_approval_external_chain",
            require_approval_reason: "External-chain mutation, replay, simulation, or audit actions require operator approval.",
        },
        AutonomousToolRequest::Emulator(_) => SafetyPolicyMetadata {
            risk_class: "device_control",
            network_intent: "none",
            credential_sensitivity: "possible",
            os_target: Some("emulator"),
            prior_observation_required: false,
            requires_approval: false,
            require_approval_code: "policy_requires_approval_device_control",
            require_approval_reason: "Device control requires operator approval.",
        },
        _ => SafetyPolicyMetadata {
            risk_class: "observe",
            network_intent: "none",
            credential_sensitivity: "low",
            os_target: None,
            prior_observation_required: false,
            requires_approval: false,
            require_approval_code: "policy_requires_approval_tool_call",
            require_approval_reason: "This tool call requires operator approval.",
        },
    }
}

fn command_family_policy_decision(
    runtime: &AutonomousToolRuntime,
    request: &AutonomousToolRequest,
) -> CommandResult<Option<(AutonomousSafetyPolicyAction, String, String)>> {
    let command_request = match request {
        AutonomousToolRequest::Command(request) => Some(request.clone()),
        AutonomousToolRequest::CommandSessionStart(request) => Some(AutonomousCommandRequest {
            argv: request.argv.clone(),
            cwd: request.cwd.clone(),
            timeout_ms: request.timeout_ms,
        }),
        AutonomousToolRequest::PowerShell(request) => Some(AutonomousCommandRequest {
            argv: vec![
                if cfg!(target_os = "windows") {
                    "powershell.exe".into()
                } else {
                    "pwsh".into()
                },
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                request.script.clone(),
            ],
            cwd: request.cwd.clone(),
            timeout_ms: request.timeout_ms,
        }),
        _ => None,
    };

    let Some(command_request) = command_request else {
        return Ok(None);
    };
    let prepared = runtime.prepare_command_request(command_request)?;
    Ok(Some(match runtime.evaluate_command_policy(prepared)? {
        CommandPolicyDecision::Allow { policy, .. } => (
            AutonomousSafetyPolicyAction::Allow,
            policy.code,
            policy.reason,
        ),
        CommandPolicyDecision::Escalate { policy, .. } => (
            AutonomousSafetyPolicyAction::RequireApproval,
            policy.code,
            policy.reason,
        ),
    }))
}

struct SafetyDecisionContext<'a> {
    tool_name: &'a str,
    metadata: SafetyPolicyMetadata,
    approval_mode: Option<RuntimeRunApprovalModeDto>,
    operator_approved: bool,
    input_sha256: &'a str,
}

fn safety_decision(
    action: AutonomousSafetyPolicyAction,
    code: impl Into<String>,
    explanation: impl Into<String>,
    context: &SafetyDecisionContext<'_>,
) -> AutonomousSafetyPolicyDecision {
    let approval_grant = context
        .operator_approved
        .then(|| AutonomousSafetyApprovalGrant {
            scope: format!("tool_call:{}", context.tool_name),
            expires: "run_end".into(),
            replay_rule: "exact_tool_call_input_sha256".into(),
            input_sha256: context.input_sha256.into(),
        });
    AutonomousSafetyPolicyDecision {
        action,
        code: code.into(),
        explanation: explanation.into(),
        tool_name: context.tool_name.into(),
        risk_class: context.metadata.risk_class.into(),
        approval_mode: context.approval_mode.clone(),
        project_trust: "imported_project".into(),
        network_intent: context.metadata.network_intent.into(),
        credential_sensitivity: context.metadata.credential_sensitivity.into(),
        os_target: context.metadata.os_target.map(str::to_owned),
        prior_observation_required: context.metadata.prior_observation_required,
        approval_grant,
    }
}

fn secret_like_tool_input(value: &JsonValue) -> bool {
    match value {
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => false,
        JsonValue::String(text) => high_confidence_secret_text(text),
        JsonValue::Array(items) => items.iter().any(secret_like_tool_input),
        JsonValue::Object(fields) => fields
            .iter()
            .any(|(key, value)| is_sensitive_argument_name(key) || secret_like_tool_input(value)),
    }
}

fn high_confidence_secret_text(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    normalized.contains("bearer ")
        || normalized.contains("sk-")
        || normalized.contains("ghp_")
        || normalized.contains("gho_")
        || normalized.contains("ghu_")
        || normalized.contains("ghs_")
        || normalized.contains("github_pat_")
        || normalized.contains("glpat-")
        || normalized.contains("xoxb-")
        || normalized.contains("xoxp-")
        || normalized.contains("-----begin")
        || normalized.contains("akia")
        || normalized.contains("aiza")
        || normalized.contains("ya29.")
        || find_prohibited_persistence_content(text).is_some()
            && (normalized.contains('=')
                || normalized.contains(':')
                || normalized.contains("token")
                || normalized.contains("password")
                || normalized.contains("private"))
}

fn repo_path_escape(request: &AutonomousToolRequest) -> Option<String> {
    repo_relative_paths(request)
        .into_iter()
        .find(|path| normalize_relative_path(path, "path").is_err())
        .map(str::to_owned)
}

fn repo_relative_paths(request: &AutonomousToolRequest) -> Vec<&str> {
    match request {
        AutonomousToolRequest::Read(request) if !request.system_path => vec![request.path.as_str()],
        AutonomousToolRequest::Search(request) => {
            optional_repo_relative_path(request.path.as_deref())
        }
        AutonomousToolRequest::Find(request) => {
            optional_repo_relative_path(request.path.as_deref())
        }
        AutonomousToolRequest::List(request) => {
            optional_repo_relative_path(request.path.as_deref())
        }
        AutonomousToolRequest::Hash(request) => vec![request.path.as_str()],
        AutonomousToolRequest::Write(request) => vec![request.path.as_str()],
        AutonomousToolRequest::Edit(request) => vec![request.path.as_str()],
        AutonomousToolRequest::Patch(request) => {
            let mut paths = optional_repo_relative_path(request.path.as_deref());
            paths.extend(
                request
                    .operations
                    .iter()
                    .map(|operation| operation.path.as_str()),
            );
            paths
        }
        AutonomousToolRequest::Delete(request) => vec![request.path.as_str()],
        AutonomousToolRequest::Rename(request) => {
            vec![request.from_path.as_str(), request.to_path.as_str()]
        }
        AutonomousToolRequest::Mkdir(request) => vec![request.path.as_str()],
        AutonomousToolRequest::NotebookEdit(request) => vec![request.path.as_str()],
        _ => Vec::new(),
    }
}

fn optional_repo_relative_path(path: Option<&str>) -> Vec<&str> {
    path.map(str::trim)
        .filter(|path| !path.is_empty())
        .into_iter()
        .collect()
}

fn is_destructive_system_operation(request: &AutonomousToolRequest) -> bool {
    let argv = match request {
        AutonomousToolRequest::Command(request) => request.argv.as_slice(),
        AutonomousToolRequest::CommandSessionStart(request) => request.argv.as_slice(),
        _ => return false,
    };
    let Some(program) = argv
        .first()
        .map(|program| executable_name(program).to_ascii_lowercase())
    else {
        return false;
    };
    matches!(
        program.as_str(),
        "sudo" | "su" | "doas" | "diskutil" | "mkfs" | "shutdown" | "reboot"
    ) || (program == "dd" && argv.iter().any(|argument| argument.starts_with("of=")))
}

pub(super) fn process_manager_policy_trace(
    action: AutonomousProcessManagerAction,
    target_ownership: Option<AutonomousProcessOwnershipScope>,
    persistent: bool,
) -> AutonomousProcessManagerPolicyTrace {
    let target_scope = target_ownership.unwrap_or(AutonomousProcessOwnershipScope::XeroOwned);
    let risk_level = match action {
        AutonomousProcessManagerAction::List
        | AutonomousProcessManagerAction::Status
        | AutonomousProcessManagerAction::Output
        | AutonomousProcessManagerAction::Digest
        | AutonomousProcessManagerAction::WaitForReady
        | AutonomousProcessManagerAction::Highlights
        | AutonomousProcessManagerAction::Env
        | AutonomousProcessManagerAction::GroupStatus
        | AutonomousProcessManagerAction::AsyncAwait
        | AutonomousProcessManagerAction::SystemProcessList
        | AutonomousProcessManagerAction::SystemProcessTree
        | AutonomousProcessManagerAction::SystemPortList => {
            AutonomousProcessActionRiskLevel::Observe
        }
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
        | AutonomousProcessManagerAction::SystemSignal
        | AutonomousProcessManagerAction::SystemKillTree
            if target_scope == AutonomousProcessOwnershipScope::External =>
        {
            AutonomousProcessActionRiskLevel::SignalExternal
        }
        AutonomousProcessManagerAction::Signal
        | AutonomousProcessManagerAction::Kill
        | AutonomousProcessManagerAction::GroupKill
        | AutonomousProcessManagerAction::AsyncCancel
        | AutonomousProcessManagerAction::Restart
        | AutonomousProcessManagerAction::SystemSignal
        | AutonomousProcessManagerAction::SystemKillTree => {
            AutonomousProcessActionRiskLevel::SignalOwned
        }
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
            "Starting or interacting with Xero-owned processes must pass the existing repo-scoped command approval policy before implementation.",
        ),
        AutonomousProcessActionRiskLevel::SignalOwned => (
            false,
            "process_policy_signal_owned",
            "Signaling Xero-owned processes is allowed after ownership verification and audit logging.",
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
            "Xero requires autonomous command requests to include a non-empty argv[0].",
        ));
    }

    if argv.iter().any(|argument| argument.contains('\0')) {
        return Err(CommandError::user_fixable(
            "autonomous_tool_command_invalid",
            "Xero refused a command that contained a NUL byte.",
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
            format!("Xero requires command timeout_ms to be between 1 and {max_timeout_ms}."),
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
                        "Xero denied the autonomous shell command under active approval mode `{}` because argument `{candidate}` escapes the imported repository root.",
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
            "Xero denied the autonomous shell command because its cwd escapes the imported repository root.",
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

fn classify_command(prepared: &PreparedCommandRequest) -> CommandClassification {
    let argv = &prepared.argv;
    let program = executable_name(&argv[0]);

    if is_shell_wrapper(program) {
        if shell_wrapper_contains_sensitive_pattern(argv) {
            return CommandClassification::Ambiguous {
                code: "policy_escalated_sensitive_shell",
                reason: format!(
                    "Xero requires operator review for shell wrapper command `{}` because the script may expand environment variables, access absolute paths, or contact external network surfaces.",
                    render_command_for_summary(argv)
                ),
            };
        }
        if shell_wrapper_contains_destructive_pattern(argv) {
            return CommandClassification::Destructive {
                code: "policy_escalated_destructive_shell",
                reason: format!(
                    "Xero requires operator review for shell wrapper command `{}` because the script text matches the destructive command classifier.",
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
                    "Xero requires operator review for `{}` because it can contact external network surfaces.",
                    render_command_for_summary(argv)
                ),
            }
        }
        "openssl" if argv.iter().any(|argument| argument == "s_client") => {
            CommandClassification::Ambiguous {
                code: "policy_escalated_network_command",
                reason: format!(
                    "Xero requires operator review for `{}` because it can contact external network surfaces.",
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
                        "Xero requires operator review for `{}` because `find -delete` is destructive.",
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
        "npm" | "pnpm" | "yarn" | "bun" => classify_package_manager_command(argv, &prepared.cwd),
        "rm" | "rmdir" | "del" | "erase" | "rd" | "mv" | "move" | "chmod" | "chown"
        | "dd" | "mkfs" | "diskutil" => CommandClassification::Destructive {
            code: "policy_escalated_destructive_command",
            reason: format!(
                "Xero requires operator review for `{}` because it matches the destructive command classifier.",
                render_command_for_summary(argv)
            ),
        },
        _ => CommandClassification::Ambiguous {
            code: "policy_escalated_ambiguous_command",
            reason: format!(
                "Xero could not classify `{}` as a repo-scoped non-destructive command, so operator review is required.",
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
                "Xero could not classify git command `{}` as non-destructive, so operator review is required.",
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
                "Xero could not classify cargo command `{}` as non-destructive, so operator review is required.",
                render_command_for_summary(argv)
            ),
        },
    }
}

fn classify_package_manager_command(argv: &[String], cwd: &Path) -> CommandClassification {
    let subcommand = argv
        .iter()
        .skip(1)
        .find(|argument| !argument.starts_with('-'));
    match subcommand.map(String::as_str) {
        Some("install" | "add" | "remove" | "unlink" | "upgrade" | "update") => {
            CommandClassification::Ambiguous {
                code: "policy_escalated_package_manager_mutation",
                reason: format!(
                    "Xero requires operator review for `{}` because package-manager mutation commands can execute install scripts, change dependency state, or contact external registries.",
                    render_command_for_summary(argv)
                ),
            }
        }
        Some(script @ ("test" | "lint" | "typecheck" | "build")) => {
            classify_repo_package_script(argv, cwd, script, true)
        }
        Some("exec") => CommandClassification::Ambiguous {
            code: "policy_escalated_package_manager_exec",
            reason: format!(
                "Xero requires operator review for `{}` because package-manager exec commands can run arbitrary local or registry-provided binaries.",
                render_command_for_summary(argv)
            ),
        },
        Some("publish") => destructive_command(argv, "package manager publish commands can affect external registries"),
        Some("run" | "run-script") => classify_package_manager_run_script(argv, cwd),
        Some(_) | None => CommandClassification::Ambiguous {
            code: "policy_escalated_ambiguous_command",
            reason: format!(
                "Xero could not classify package-manager command `{}` as non-destructive, so operator review is required.",
                render_command_for_summary(argv)
            ),
        },
    }
}

fn classify_package_manager_run_script(argv: &[String], cwd: &Path) -> CommandClassification {
    let Some(script_name) = package_manager_run_script_name(argv) else {
        return CommandClassification::Ambiguous {
            code: "policy_escalated_package_manager_run",
            reason: format!(
                "Xero requires operator review for `{}` because the package-manager script name could not be identified.",
                render_command_for_summary(argv)
            ),
        };
    };

    classify_repo_package_script(argv, cwd, script_name, false)
}

fn package_manager_run_script_name(argv: &[String]) -> Option<&str> {
    let mut seen_run = false;
    for argument in argv.iter().skip(1) {
        if !seen_run {
            if matches!(argument.as_str(), "run" | "run-script") {
                seen_run = true;
            }
            continue;
        }
        if argument == "--" || argument.starts_with('-') {
            continue;
        }
        return Some(argument.as_str());
    }
    None
}

fn classify_repo_package_script(
    argv: &[String],
    cwd: &Path,
    script_name: &str,
    direct_script_command: bool,
) -> CommandClassification {
    if !is_safe_package_script_name(script_name) {
        return CommandClassification::Ambiguous {
            code: "policy_escalated_package_manager_run",
            reason: format!(
                "Xero requires operator review for `{}` because package script `{script_name}` is not in the repo-local verification allowlist.",
                render_command_for_summary(argv)
            ),
        };
    }

    let Some(script) = package_json_script(cwd, script_name) else {
        if direct_script_command {
            return safe_command(argv);
        }
        return CommandClassification::Ambiguous {
            code: "policy_escalated_package_manager_run_missing_script",
            reason: format!(
                "Xero requires operator review for `{}` because package.json in `{}` does not define script `{script_name}`.",
                render_command_for_summary(argv),
                cwd.display()
            ),
        };
    };

    let shell_argv = vec!["sh".to_string(), "-c".to_string(), script];
    if shell_wrapper_contains_destructive_pattern(&shell_argv) {
        return CommandClassification::Destructive {
            code: "policy_escalated_destructive_package_script",
            reason: format!(
                "Xero requires operator review for `{}` because package script `{script_name}` contains destructive shell patterns.",
                render_command_for_summary(argv)
            ),
        };
    }
    if shell_wrapper_contains_sensitive_pattern(&shell_argv) {
        return CommandClassification::Ambiguous {
            code: "policy_escalated_sensitive_package_script",
            reason: format!(
                "Xero requires operator review for `{}` because package script `{script_name}` may expand secrets, access absolute paths, or contact external network surfaces.",
                render_command_for_summary(argv)
            ),
        };
    }

    CommandClassification::Safe(format!(
        "Active approval mode `yolo` allowed repo-local package script `{script_name}` via `{}` after package.json introspection classified the script as verification-safe.",
        render_command_for_summary(argv)
    ))
}

fn is_safe_package_script_name(script_name: &str) -> bool {
    matches!(
        script_name,
        "test" | "tests" | "lint" | "typecheck" | "check" | "build" | "rust:test"
    )
}

fn package_json_script(cwd: &Path, script_name: &str) -> Option<String> {
    let package_json = cwd.join("package.json");
    let bytes = fs::read(package_json).ok()?;
    let value = serde_json::from_slice::<serde_json::Value>(&bytes).ok()?;
    value
        .get("scripts")
        .and_then(serde_json::Value::as_object)
        .and_then(|scripts| scripts.get(script_name))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
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
            "Xero requires operator review for `{}` because {reason}.",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{
        RuntimeAgentIdDto, RuntimeRunActiveControlSnapshotDto, RuntimeRunControlStateDto,
    };
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn package_manager_run_allows_introspected_verification_script() {
        let tempdir = tempdir().expect("tempdir");
        fs::write(
            tempdir.path().join("package.json"),
            r#"{"scripts":{"test":"vitest run","build":"vite build"}}"#,
        )
        .expect("package");
        let prepared = prepared_command(tempdir.path(), ["npm", "run", "test"]);

        let decision = classify_command(&prepared);

        match decision {
            CommandClassification::Safe(reason) => {
                assert!(reason.contains("package.json introspection"));
                assert!(reason.contains("test"));
            }
            other => panic!("expected safe script, got {other:?}"),
        }
    }

    #[test]
    fn package_manager_run_escalates_destructive_allowed_script() {
        let tempdir = tempdir().expect("tempdir");
        fs::write(
            tempdir.path().join("package.json"),
            r#"{"scripts":{"build":"rm -rf dist"}}"#,
        )
        .expect("package");
        let prepared = prepared_command(tempdir.path(), ["npm", "run", "build"]);

        let decision = classify_command(&prepared);

        match decision {
            CommandClassification::Destructive { code, reason } => {
                assert_eq!(code, "policy_escalated_destructive_package_script");
                assert!(reason.contains("build"));
            }
            other => panic!("expected destructive script, got {other:?}"),
        }
    }

    #[test]
    fn package_manager_run_escalates_non_allowlisted_script() {
        let tempdir = tempdir().expect("tempdir");
        fs::write(
            tempdir.path().join("package.json"),
            r#"{"scripts":{"deploy":"vite build"}}"#,
        )
        .expect("package");
        let prepared = prepared_command(tempdir.path(), ["npm", "run", "deploy"]);

        let decision = classify_command(&prepared);

        match decision {
            CommandClassification::Ambiguous { code, reason } => {
                assert_eq!(code, "policy_escalated_package_manager_run");
                assert!(reason.contains("verification allowlist"));
            }
            other => panic!("expected ambiguous script, got {other:?}"),
        }
    }

    #[test]
    fn safety_policy_denies_secret_like_tool_input() {
        let tempdir = tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path(), RuntimeRunApprovalModeDto::Yolo);
        let request =
            AutonomousToolRequest::ToolAccess(super::super::AutonomousToolAccessRequest {
                action: super::super::AutonomousToolAccessAction::Request,
                groups: Vec::new(),
                tools: Vec::new(),
                reason: Some("use token sk-test-secret".into()),
            });

        let decision = runtime
            .evaluate_safety_policy(
                "tool_access",
                &json!({"reason": "use token sk-test-secret"}),
                &request,
                false,
                "input-hash",
            )
            .expect("policy");

        assert_eq!(decision.action, AutonomousSafetyPolicyAction::Deny);
        assert_eq!(decision.code, "policy_denied_secret_like_tool_input");
    }

    #[test]
    fn safety_policy_denies_repo_path_escape() {
        let tempdir = tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path(), RuntimeRunApprovalModeDto::Yolo);
        let request = AutonomousToolRequest::Write(super::super::AutonomousWriteRequest {
            path: "../outside.txt".into(),
            content: "hello".into(),
        });

        let decision = runtime
            .evaluate_safety_policy(
                "write",
                &json!({"path": "../outside.txt", "content": "hello"}),
                &request,
                false,
                "input-hash",
            )
            .expect("policy");

        assert_eq!(decision.action, AutonomousSafetyPolicyAction::Deny);
        assert_eq!(decision.code, "policy_denied_path_escape");
    }

    #[test]
    fn safety_policy_allows_blank_optional_observe_scope_as_repo_root() {
        let tempdir = tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path(), RuntimeRunApprovalModeDto::Yolo);
        let request = AutonomousToolRequest::List(super::super::AutonomousListRequest {
            path: Some("".into()),
            max_depth: Some(2),
        });

        let decision = runtime
            .evaluate_safety_policy(
                "list",
                &json!({"path": "", "maxDepth": 2}),
                &request,
                false,
                "input-hash",
            )
            .expect("policy");

        assert_eq!(decision.action, AutonomousSafetyPolicyAction::Allow);
    }

    #[test]
    fn safety_policy_uses_command_approval_mode() {
        let tempdir = tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path(), RuntimeRunApprovalModeDto::Suggest);
        let request = AutonomousToolRequest::Command(AutonomousCommandRequest {
            argv: vec!["echo".into(), "hello".into()],
            cwd: None,
            timeout_ms: None,
        });

        let decision = runtime
            .evaluate_safety_policy(
                "command",
                &json!({"argv": ["echo", "hello"]}),
                &request,
                false,
                "input-hash",
            )
            .expect("policy");

        assert_eq!(
            decision.action,
            AutonomousSafetyPolicyAction::RequireApproval
        );
        assert_eq!(decision.code, "policy_escalated_approval_mode");
    }

    fn prepared_command<const N: usize>(cwd: &Path, argv: [&str; N]) -> PreparedCommandRequest {
        PreparedCommandRequest {
            argv: argv.into_iter().map(str::to_owned).collect(),
            cwd_relative: None,
            cwd: cwd.to_path_buf(),
            timeout_ms: DEFAULT_COMMAND_TIMEOUT_MS,
        }
    }

    fn test_runtime(
        repo_root: &Path,
        approval_mode: RuntimeRunApprovalModeDto,
    ) -> AutonomousToolRuntime {
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
                    approval_mode,
                    plan_mode_required: false,
                    revision: 1,
                    applied_at: "2026-04-30T00:00:00Z".into(),
                },
                pending: None,
            })
    }
}
