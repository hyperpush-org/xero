use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{
    filesystem::expand_system_path,
    repo_scope::{is_current_directory_path, normalize_relative_path},
    tool_allowed_for_runtime_agent_with_policy, AutonomousBrowserAction,
    AutonomousCommandPolicyOutcome, AutonomousCommandPolicyProfile, AutonomousCommandPolicyTrace,
    AutonomousCommandRequest, AutonomousDesktopControlAction, AutonomousDesktopObserveAction,
    AutonomousFsTransactionAction, AutonomousMacosAutomationAction, AutonomousMcpAction,
    AutonomousProcessActionRiskLevel, AutonomousProcessManagerAction,
    AutonomousProcessManagerPolicyTrace, AutonomousProcessOwnershipScope,
    AutonomousProjectContextAction, AutonomousSafetyApprovalGrant, AutonomousSafetyPolicyAction,
    AutonomousSafetyPolicyDecision, AutonomousSystemDiagnosticsAction,
    AutonomousSystemDiagnosticsPolicyTrace, AutonomousToolRequest, AutonomousToolRuntime,
    AutonomousWorkflowDefinitionAction, AUTONOMOUS_TOOL_COMMAND, AUTONOMOUS_TOOL_COMMAND_PROBE,
    AUTONOMOUS_TOOL_COMMAND_VERIFY, AUTONOMOUS_TOOL_HOST_COMMAND, AUTONOMOUS_TOOL_PROCESS_MANAGER,
    DEFAULT_COMMAND_TIMEOUT_MS,
};
use crate::runtime::redaction::{
    high_confidence_secret_text, is_sensitive_argument_name, render_command_for_persistence,
};
use crate::{
    auth::now_timestamp,
    commands::{
        validate_non_empty, CommandError, CommandErrorClass, CommandResult,
        RuntimeRunApprovalModeDto,
    },
    db::project_store,
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
        let mut metadata = safety_policy_metadata(request);
        if self.linked_context_system_read_without_approval(request) {
            metadata.requires_approval = false;
            metadata.require_approval_code = "policy_allowed_linked_context_system_read";
            metadata.require_approval_reason =
                "Reading an attached linked context path does not require operator approval.";
        }
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

        if !tool_allowed_for_runtime_agent_with_policy(
            self.active_runtime_agent_id(),
            tool_name,
            self.agent_tool_policy.as_ref(),
        ) {
            return Ok(safety_decision(
                AutonomousSafetyPolicyAction::Deny,
                "policy_denied_tool_for_agent",
                format!(
                    "The {} agent is not allowed to call `{tool_name}`.",
                    self.active_runtime_agent_id().label()
                ),
                &context,
            ));
        }

        if secret_like_tool_input(raw_input) {
            return Ok(safety_decision(
                AutonomousSafetyPolicyAction::Deny,
                "policy_denied_secret_like_tool_input",
                "Xero denied the tool call because its arguments contain credential-like material. Secret-bearing tool inputs are not persisted, replayed, or sent back to the model.",
                &context,
            ));
        }

        if let Some(path) = self.repo_path_escape(request) {
            return Ok(safety_decision(
                AutonomousSafetyPolicyAction::Deny,
                "policy_denied_path_escape",
                format!(
                    "Xero denied the tool call because path `{path}` escapes the imported repository root and linked context paths."
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

        if project_context_action_mutates_app_state(request)
            && !self.active_runtime_agent_id().allows_engineering_tools()
        {
            return Ok(safety_decision(
                AutonomousSafetyPolicyAction::Deny,
                "policy_denied_project_context_mutation_for_agent",
                format!(
                    "The {} agent cannot mutate durable project context.",
                    self.active_runtime_agent_id().label()
                ),
                &context,
            ));
        }

        if request_requires_mailbox_check(self, tool_name, request)? {
            let mailbox_scope_paths = mailbox_gate_scope_paths(request);
            if let Some((code, explanation)) =
                mailbox_check_policy_denial(self, &mailbox_scope_paths)?
            {
                return Ok(safety_decision(
                    AutonomousSafetyPolicyAction::Deny,
                    code,
                    explanation,
                    &context,
                ));
            }
        }

        let command_decision = command_family_policy_decision(self, tool_name, request)?;
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

    fn linked_context_system_read_without_approval(&self, request: &AutonomousToolRequest) -> bool {
        let AutonomousToolRequest::Read(request) = request else {
            return false;
        };
        if !request.system_path {
            return false;
        }
        let Ok(expanded) = expand_system_path(&request.path) else {
            return false;
        };
        if !expanded.is_absolute() {
            return false;
        }
        self.linked_absolute_tool_path_allowed(&expanded)
    }

    fn repo_path_escape(&self, request: &AutonomousToolRequest) -> Option<String> {
        let allow_linked_read_paths = request_allows_linked_absolute_paths(request);
        repo_relative_paths(request)
            .into_iter()
            .find(|path| {
                let trimmed = path.trim();
                if allow_linked_read_paths && should_treat_policy_path_as_absolute(trimmed) {
                    let Ok(expanded) = expand_system_path(trimmed) else {
                        return true;
                    };
                    return !self.linked_absolute_tool_path_allowed(&expanded);
                }

                matches!(
                    normalize_relative_path(trimmed, "path"),
                    Err(error) if error.class == CommandErrorClass::PolicyDenied
                )
            })
            .map(str::to_owned)
    }

    fn linked_absolute_tool_path_allowed(&self, path: &Path) -> bool {
        if !path.is_absolute() {
            return false;
        }
        let Ok(resolved) = fs::canonicalize(path) else {
            return false;
        };
        self.is_within_linked_read_root(&resolved)
    }

    pub(super) fn enforce_mailbox_check_before_mutation(
        &self,
        tool_name: &str,
        request: &AutonomousToolRequest,
    ) -> CommandResult<()> {
        if request_requires_mailbox_check(self, tool_name, request)? {
            let mailbox_scope_paths = mailbox_gate_scope_paths(request);
            if let Some((code, explanation)) =
                mailbox_check_policy_denial(self, &mailbox_scope_paths)?
            {
                return Err(CommandError::new(
                    code,
                    CommandErrorClass::PolicyDenied,
                    explanation,
                    false,
                ));
            }
        }
        Ok(())
    }

    pub(super) fn prepare_command_request(
        &self,
        request: AutonomousCommandRequest,
    ) -> CommandResult<PreparedCommandRequest> {
        let argv = normalize_command_argv(&request.argv)?;
        let cwd_relative = match request.cwd.as_deref() {
            Some(cwd) => normalize_command_cwd(cwd)?,
            None => None,
        };
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
        self.evaluate_command_policy_for_tool(AUTONOMOUS_TOOL_COMMAND, prepared)
    }

    pub(super) fn evaluate_command_policy_for_tool(
        &self,
        tool_name: &str,
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

        self.validate_repo_scoped_arguments(&prepared, active.approval_mode.clone())?;

        let policy = match classify_command(&prepared) {
            CommandClassification::Safe { profile, reason } => policy_trace(
                AutonomousCommandPolicyOutcome::Allowed,
                active.approval_mode.clone(),
                profile,
                "policy_allowed_repo_scoped_command",
                reason,
            ),
            CommandClassification::Escalated {
                profile,
                code,
                reason,
            } if active.approval_mode == RuntimeRunApprovalModeDto::Yolo
                && tool_name != AUTONOMOUS_TOOL_PROCESS_MANAGER =>
            {
                policy_trace(
                AutonomousCommandPolicyOutcome::Allowed,
                active.approval_mode.clone(),
                profile,
                "policy_allowed_full_access_command",
                format!(
                    "Active approval mode `{}` allowed repo-scoped command `{}` without command-classifier restrictions. Classifier `{code}` was recorded for audit only: {reason}",
                    approval_mode_label(&active.approval_mode),
                    render_command_for_summary(&prepared.argv)
                ),
                )
            }
            CommandClassification::Escalated {
                profile,
                code,
                reason,
            } => {
                return Ok(CommandPolicyDecision::Escalate {
                    prepared,
                    policy: policy_trace(
                        AutonomousCommandPolicyOutcome::Escalated,
                        active.approval_mode.clone(),
                        profile,
                        code,
                        reason,
                    ),
                });
            }
        };

        if active.approval_mode != RuntimeRunApprovalModeDto::Yolo {
            if command_tool_can_run_without_operator_review(tool_name, &prepared, &policy) {
                return Ok(CommandPolicyDecision::Allow { prepared, policy });
            }
            if let Some(policy) = command_tool_scope_escalation(tool_name, &prepared, &policy) {
                return Ok(CommandPolicyDecision::Escalate { prepared, policy });
            }
            let policy = policy_trace(
                AutonomousCommandPolicyOutcome::Escalated,
                active.approval_mode.clone(),
                AutonomousCommandPolicyProfile::GeneralExecution,
                "policy_escalated_approval_mode",
                format!(
                    "Active approval mode `{}` requires operator review before autonomous shell commands can run.",
                    approval_mode_label(&active.approval_mode)
                ),
            );
            return Ok(CommandPolicyDecision::Escalate { prepared, policy });
        }

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
        AutonomousToolRequest::ProjectContext(request) => {
            if project_context_action_is_read(request.action) {
                SafetyPolicyMetadata {
                    risk_class: "project_context_read",
                    network_intent: "none",
                    credential_sensitivity: "low",
                    os_target: None,
                    prior_observation_required: false,
                    requires_approval: false,
                    require_approval_code: "policy_requires_approval_project_context_read",
                    require_approval_reason: "Project-context reads do not require operator approval.",
                }
            } else {
                SafetyPolicyMetadata {
                    risk_class: "runtime_state",
                    network_intent: "none",
                    credential_sensitivity: "possible",
                    os_target: None,
                    prior_observation_required: false,
                    requires_approval: false,
                    require_approval_code: "policy_requires_approval_project_context_mutation",
                    require_approval_reason: "Durable project-context mutations require runtime-agent authority.",
                }
            }
        }
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
        AutonomousToolRequest::Browser(request) => {
            let requires_approval = browser_action_requires_approval(&request.action);
            if browser_action_is_observe(&request.action) {
                SafetyPolicyMetadata {
                    risk_class: "browser_observe",
                    network_intent: "browser",
                    credential_sensitivity: "possible",
                    os_target: Some("browser"),
                    prior_observation_required: false,
                    requires_approval,
                    require_approval_code: "policy_requires_approval_browser_observe",
                    require_approval_reason: if requires_approval {
                        "This browser observation reads or persists sensitive browser evidence and requires operator approval."
                    } else {
                        "Browser observation does not require operator approval."
                    },
                }
            } else {
                SafetyPolicyMetadata {
                    risk_class: browser_action_risk_class(&request.action),
                    network_intent: "browser",
                    credential_sensitivity: "possible",
                    os_target: Some("browser"),
                    prior_observation_required: false,
                    requires_approval,
                    require_approval_code: "policy_requires_approval_browser_control",
                    require_approval_reason: if requires_approval {
                        "This browser action transfers files, changes credential/browser state, intercepts network traffic, emits durable evidence, or exposes an external bridge and requires operator approval."
                    } else {
                        "Non-sensitive browser control does not require operator approval."
                    },
                }
            }
        }
        AutonomousToolRequest::MacosAutomation(request) => {
            let requires_approval = macos_automation_action_requires_approval(request.action);
            SafetyPolicyMetadata {
                risk_class: macos_automation_risk_class(request.action),
                network_intent: "none",
                credential_sensitivity: if matches!(
                    request.action,
                    AutonomousMacosAutomationAction::MacPermissions
                        | AutonomousMacosAutomationAction::MacAppList
                        | AutonomousMacosAutomationAction::MacWindowList
                ) {
                    "low"
                } else {
                    "possible"
                },
                os_target: Some("macos"),
                prior_observation_required: false,
                requires_approval,
                require_approval_code: if requires_approval {
                    "policy_requires_approval_destructive_os_automation"
                } else {
                    "policy_allows_non_destructive_os_automation"
                },
                require_approval_reason: if requires_approval {
                    "Quitting an app can lose unsaved work and requires operator approval."
                } else {
                    "Read-only and non-destructive macOS automation does not require operator approval."
                },
            }
        }
        AutonomousToolRequest::DesktopObserve(request) => {
            let sensitive = matches!(
                request.action,
                AutonomousDesktopObserveAction::Screenshot
                    | AutonomousDesktopObserveAction::AccessibilitySnapshot
                    | AutonomousDesktopObserveAction::OcrSnapshot
                    | AutonomousDesktopObserveAction::ElementAtPoint
                    | AutonomousDesktopObserveAction::ClipboardReadText
                    | AutonomousDesktopObserveAction::ClipboardReadHtml
                    | AutonomousDesktopObserveAction::ClipboardReadRtf
                    | AutonomousDesktopObserveAction::ClipboardReadImage
                    | AutonomousDesktopObserveAction::ClipboardReadFiles
                    | AutonomousDesktopObserveAction::NotificationSnapshot
            );
            let requires_approval = matches!(
                request.action,
                AutonomousDesktopObserveAction::ClipboardReadText
                    | AutonomousDesktopObserveAction::ClipboardReadHtml
                    | AutonomousDesktopObserveAction::ClipboardReadRtf
                    | AutonomousDesktopObserveAction::ClipboardReadImage
                    | AutonomousDesktopObserveAction::ClipboardReadFiles
                    | AutonomousDesktopObserveAction::NotificationSnapshot
            );
            SafetyPolicyMetadata {
                risk_class: if sensitive {
                    "desktop_observe_sensitive"
                } else {
                    "desktop_observe"
                },
                network_intent: "none",
                credential_sensitivity: if sensitive { "possible" } else { "low" },
                os_target: Some("desktop"),
                prior_observation_required: false,
                requires_approval,
                require_approval_code: if requires_approval {
                    "policy_requires_approval_sensitive_desktop_observe"
                } else {
                    "policy_allows_desktop_observe"
                },
                require_approval_reason: if requires_approval {
                    "Reading system clipboard or notification content can expose sensitive local data and requires operator approval."
                } else {
                    "Desktop observation is read-only and does not require operator approval."
                },
            }
        }
        AutonomousToolRequest::DesktopControl(request) => {
            let cancel_only = matches!(
                request.action,
                AutonomousDesktopControlAction::CancelCurrentAction
            );
            let requires_approval = desktop_control_action_requires_approval(&request.action);
            SafetyPolicyMetadata {
                risk_class: if requires_approval {
                    "desktop_control_destructive"
                } else if cancel_only {
                    "desktop_control_cancel"
                } else {
                    "desktop_control"
                },
                network_intent: "none",
                credential_sensitivity: "possible",
                os_target: Some("desktop"),
                prior_observation_required: !cancel_only,
                requires_approval,
                require_approval_code: if requires_approval {
                    "policy_requires_approval_destructive_desktop_control"
                } else {
                    "policy_allows_non_destructive_desktop_control"
                },
                require_approval_reason: if requires_approval {
                    "This desktop action can affect apps or expose local resources and requires operator approval."
                } else {
                    "Non-destructive desktop control does not require operator approval."
                },
            }
        }
        AutonomousToolRequest::DesktopStream(_) => {
            SafetyPolicyMetadata {
                risk_class: "desktop_stream",
                network_intent: "stream_media",
                credential_sensitivity: "possible",
                os_target: Some("desktop"),
                prior_observation_required: false,
                requires_approval: false,
                require_approval_code: "policy_allows_desktop_stream",
                require_approval_reason: "Desktop streaming is read-only and does not require operator approval.",
            }
        }
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
        AutonomousToolRequest::HostCommand(_) => SafetyPolicyMetadata {
            risk_class: "host_admin",
            network_intent: "command_dependent",
            credential_sensitivity: "possible",
            os_target: Some("host"),
            prior_observation_required: false,
            requires_approval: true,
            require_approval_code: "policy_requires_approval_host_command",
            require_approval_reason:
                "Host administration requires local Owner Admin mode and per-command approval.",
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
        AutonomousToolRequest::SystemDiagnostics(request) => {
            let trace = system_diagnostics_policy_trace(request.action);
            let risk_class = match request.action {
                AutonomousSystemDiagnosticsAction::ProcessSample => "system_profile",
                AutonomousSystemDiagnosticsAction::MacosAccessibilitySnapshot => "os_automation",
                _ => "system_read",
            };
            SafetyPolicyMetadata {
                risk_class,
                network_intent: "none",
                credential_sensitivity: "possible",
                os_target: Some(match request.action {
                    AutonomousSystemDiagnosticsAction::MacosAccessibilitySnapshot => "macos",
                    _ => "process",
                }),
                prior_observation_required: false,
                requires_approval: trace.approval_required,
                require_approval_code: "policy_requires_approval_system_diagnostics",
                require_approval_reason: "This diagnostics action requires operator approval.",
            }
        }
        AutonomousToolRequest::Edit(_)
        | AutonomousToolRequest::Write(_)
        | AutonomousToolRequest::Patch(_)
        | AutonomousToolRequest::Copy(_)
        | AutonomousToolRequest::FsTransaction(_)
        | AutonomousToolRequest::JsonEdit(_)
        | AutonomousToolRequest::TomlEdit(_)
        | AutonomousToolRequest::YamlEdit(_)
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
        AutonomousToolRequest::Mcp(request) if mcp_action_is_observe(request.action) => {
            SafetyPolicyMetadata {
                risk_class: "external_capability_observe",
                network_intent: "external_capability_dependent",
                credential_sensitivity: "possible",
                os_target: None,
                prior_observation_required: false,
                requires_approval: false,
                require_approval_code: "policy_requires_approval_external_capability_observe",
                require_approval_reason: "External capability observation does not require operator approval.",
            }
        }
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
        AutonomousToolRequest::WorkflowDefinition(request) => {
            let requires_approval = matches!(
                request.action,
                AutonomousWorkflowDefinitionAction::Save
                    | AutonomousWorkflowDefinitionAction::Update
            );
            SafetyPolicyMetadata {
                risk_class: "workflow_definition_state",
                network_intent: "none",
                credential_sensitivity: "possible",
                os_target: None,
                prior_observation_required: false,
                requires_approval,
                require_approval_code: "policy_requires_approval_workflow_definition_mutation",
                require_approval_reason: "Saving or updating Workflow definitions requires explicit operator approval.",
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

fn macos_automation_action_requires_approval(action: AutonomousMacosAutomationAction) -> bool {
    matches!(action, AutonomousMacosAutomationAction::MacAppQuit)
}

fn macos_automation_risk_class(action: AutonomousMacosAutomationAction) -> &'static str {
    match action {
        AutonomousMacosAutomationAction::MacPermissions
        | AutonomousMacosAutomationAction::MacAppList
        | AutonomousMacosAutomationAction::MacWindowList => "os_observe",
        AutonomousMacosAutomationAction::MacScreenshot => "desktop_observe_sensitive",
        AutonomousMacosAutomationAction::MacAppQuit => "os_destructive_control",
        AutonomousMacosAutomationAction::MacAppLaunch
        | AutonomousMacosAutomationAction::MacAppActivate
        | AutonomousMacosAutomationAction::MacWindowFocus => "os_control",
    }
}

fn desktop_control_action_requires_approval(action: &AutonomousDesktopControlAction) -> bool {
    matches!(
        action,
        AutonomousDesktopControlAction::QuitApp
            | AutonomousDesktopControlAction::WindowClose
            | AutonomousDesktopControlAction::ClipboardWriteHtml
            | AutonomousDesktopControlAction::ClipboardWriteRtf
            | AutonomousDesktopControlAction::ClipboardWriteImage
            | AutonomousDesktopControlAction::ClipboardWriteFiles
            | AutonomousDesktopControlAction::FileDrop
    )
}

fn project_context_action_mutates_app_state(request: &AutonomousToolRequest) -> bool {
    matches!(
        request,
        AutonomousToolRequest::ProjectContext(request)
            if !project_context_action_is_read(request.action)
    )
}

fn project_context_action_is_read(action: AutonomousProjectContextAction) -> bool {
    matches!(
        action,
        AutonomousProjectContextAction::SearchProjectRecords
            | AutonomousProjectContextAction::SearchApprovedMemory
            | AutonomousProjectContextAction::GetProjectRecord
            | AutonomousProjectContextAction::GetMemory
            | AutonomousProjectContextAction::ListRecentHandoffs
            | AutonomousProjectContextAction::ListActiveDecisionsConstraints
            | AutonomousProjectContextAction::ListOpenQuestionsBlockers
            | AutonomousProjectContextAction::ExplainCurrentContextPackage
    )
}

fn request_requires_mailbox_check(
    runtime: &AutonomousToolRuntime,
    tool_name: &str,
    request: &AutonomousToolRequest,
) -> CommandResult<bool> {
    if repository_write_request(request) {
        return Ok(true);
    }

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
        return Ok(false);
    };
    let prepared = runtime.prepare_command_request(command_request)?;
    let profile = match classify_command(&prepared) {
        CommandClassification::Safe { profile, .. } => profile,
        CommandClassification::Escalated { profile, .. } => profile,
    };
    if matches!(
        profile,
        AutonomousCommandPolicyProfile::ReadOnlyVerification
    ) {
        return Ok(false);
    }
    if let Some(policy) = command_tool_scope_escalation(
        tool_name,
        &prepared,
        &policy_trace(
            AutonomousCommandPolicyOutcome::Allowed,
            RuntimeRunApprovalModeDto::Yolo,
            profile,
            "mailbox_gate_command_scope_probe",
            "Classified command for mailbox mutation gating.",
        ),
    ) {
        return Ok(policy.profile != AutonomousCommandPolicyProfile::ReadOnlyVerification);
    }
    Ok(true)
}

fn repository_write_request(request: &AutonomousToolRequest) -> bool {
    matches!(
        request,
        AutonomousToolRequest::Edit(_)
            | AutonomousToolRequest::Write(_)
            | AutonomousToolRequest::Patch(_)
            | AutonomousToolRequest::Copy(_)
            | AutonomousToolRequest::FsTransaction(_)
            | AutonomousToolRequest::JsonEdit(_)
            | AutonomousToolRequest::TomlEdit(_)
            | AutonomousToolRequest::YamlEdit(_)
            | AutonomousToolRequest::Delete(_)
            | AutonomousToolRequest::Rename(_)
            | AutonomousToolRequest::Mkdir(_)
            | AutonomousToolRequest::NotebookEdit(_)
    )
}

fn mailbox_gate_scope_paths(request: &AutonomousToolRequest) -> Vec<String> {
    let mut paths = Vec::new();
    match request {
        AutonomousToolRequest::Edit(request) => paths.push(request.path.clone()),
        AutonomousToolRequest::Write(request) => paths.push(request.path.clone()),
        AutonomousToolRequest::Patch(request) => {
            if let Some(path) = request.path.clone() {
                paths.push(path);
            }
            paths.extend(
                request
                    .operations
                    .iter()
                    .map(|operation| operation.path.clone()),
            );
        }
        AutonomousToolRequest::Copy(request) => paths.push(request.to.clone()),
        AutonomousToolRequest::FsTransaction(request) => {
            for operation in &request.operations {
                if let Some(path) = operation.path.clone() {
                    paths.push(path);
                }
                if let Some(to) = operation.to.clone() {
                    paths.push(to);
                }
                if let Some(to_path) = operation.to_path.clone() {
                    paths.push(to_path);
                }
                if matches!(
                    operation.action,
                    AutonomousFsTransactionAction::Rename
                        | AutonomousFsTransactionAction::DeleteFile
                        | AutonomousFsTransactionAction::DeleteDirectory
                ) {
                    if let Some(from) = operation.from.clone() {
                        paths.push(from);
                    }
                    if let Some(from_path) = operation.from_path.clone() {
                        paths.push(from_path);
                    }
                }
            }
        }
        AutonomousToolRequest::JsonEdit(request)
        | AutonomousToolRequest::TomlEdit(request)
        | AutonomousToolRequest::YamlEdit(request) => paths.push(request.path.clone()),
        AutonomousToolRequest::Delete(request) => paths.push(request.path.clone()),
        AutonomousToolRequest::Rename(request) => {
            paths.push(request.from_path.clone());
            paths.push(request.to_path.clone());
        }
        AutonomousToolRequest::Mkdir(request) => paths.push(request.path.clone()),
        AutonomousToolRequest::NotebookEdit(request) => paths.push(request.path.clone()),
        _ => {}
    }
    paths
        .into_iter()
        .map(|path| path.trim().to_owned())
        .filter(|path| !path.is_empty())
        .collect()
}

fn mailbox_check_policy_denial(
    runtime: &AutonomousToolRuntime,
    paths: &[String],
) -> CommandResult<Option<(&'static str, String)>> {
    let Some(run_context) = runtime.agent_run_context() else {
        return Ok(None);
    };
    let now = now_timestamp();
    let status = project_store::agent_mailbox_mutation_gate_status(
        runtime.repo_root(),
        &run_context.project_id,
        &run_context.run_id,
        &now,
        paths,
    )?;
    if !status.requires_mailbox_check() {
        return Ok(None);
    }

    let freshness = match status.checked_at.as_deref() {
        Some(checked_at) => format!(
            "the last mailbox check at {checked_at} is stale for the current coordination state"
        ),
        None => "this run has not checked its mailbox for the current coordination state".into(),
    };
    let retry_guidance = mailbox_check_retry_guidance(paths);
    Ok(Some((
        "policy_requires_mailbox_check_before_mutation",
        format!(
            "Xero denied this project-changing mutation because {freshness} while {} same-project sibling run(s) are active. {retry_guidance}",
            status.active_sibling_count
        ),
    )))
}

fn mailbox_check_retry_guidance(paths: &[String]) -> String {
    if paths.is_empty() {
        "Call `agent_coordination` with action `check_inbox_status` if you want metadata first, or action `read_inbox` unfiltered, review the temporary mailbox, then retry the mutation."
            .to_owned()
    } else {
        format!(
            "Call `agent_coordination` with action `check_inbox_status` and `paths`, or action `read_inbox` with `paths: [{}]`, review the scoped temporary mailbox, then retry the mutation.",
            paths
                .iter()
                .map(|path| format!("\"{path}\""))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn browser_action_is_observe(action: &AutonomousBrowserAction) -> bool {
    if let AutonomousBrowserAction::InAppCdpFacade { method, .. } = action {
        return in_app_cdp_facade_method_is_observe(method);
    }
    if let AutonomousBrowserAction::ActionCache { command, .. } = action {
        return matches!(command.as_str(), "stats" | "list" | "get");
    }
    matches!(
        action,
        AutonomousBrowserAction::Health
            | AutonomousBrowserAction::Capabilities { .. }
            | AutonomousBrowserAction::PageList { .. }
            | AutonomousBrowserAction::ReadText { .. }
            | AutonomousBrowserAction::Source { .. }
            | AutonomousBrowserAction::Query { .. }
            | AutonomousBrowserAction::Snapshot { .. }
            | AutonomousBrowserAction::GetRef { .. }
            | AutonomousBrowserAction::WaitForSelector { .. }
            | AutonomousBrowserAction::WaitForLoad { .. }
            | AutonomousBrowserAction::WaitFor { .. }
            | AutonomousBrowserAction::Assert { .. }
            | AutonomousBrowserAction::CurrentUrl
            | AutonomousBrowserAction::HistoryState
            | AutonomousBrowserAction::Screenshot
            | AutonomousBrowserAction::CookiesGet
            | AutonomousBrowserAction::StorageRead { .. }
            | AutonomousBrowserAction::ConsoleLogs { .. }
            | AutonomousBrowserAction::NetworkSummary { .. }
            | AutonomousBrowserAction::AccessibilityTree { .. }
            | AutonomousBrowserAction::StateSnapshot { .. }
            | AutonomousBrowserAction::FindBest { .. }
            | AutonomousBrowserAction::ActionCache { .. }
            | AutonomousBrowserAction::AnalyzeForm { .. }
            | AutonomousBrowserAction::FrameList { .. }
            | AutonomousBrowserAction::DialogList { .. }
            | AutonomousBrowserAction::DownloadList { .. }
            | AutonomousBrowserAction::TraceStatus { .. }
            | AutonomousBrowserAction::VisualBaselineList { .. }
            | AutonomousBrowserAction::EmulationState { .. }
            | AutonomousBrowserAction::Extract { .. }
            | AutonomousBrowserAction::FrameState { .. }
            | AutonomousBrowserAction::VaultList { .. }
            | AutonomousBrowserAction::AuthProfileList { .. }
            | AutonomousBrowserAction::ViewerState { .. }
            | AutonomousBrowserAction::BrowserResource { .. }
            | AutonomousBrowserAction::BrowserPrompt { .. }
            | AutonomousBrowserAction::ValidateBundle { .. }
            | AutonomousBrowserAction::Timeline { .. }
            | AutonomousBrowserAction::PromptInjectionScan { .. }
            | AutonomousBrowserAction::HarnessExtensionContract
            | AutonomousBrowserAction::TabList
    )
}

fn browser_action_requires_approval(action: &AutonomousBrowserAction) -> bool {
    if let AutonomousBrowserAction::InAppCdpFacade { method, .. } = action {
        return !in_app_cdp_facade_method_is_observe(method);
    }
    if let AutonomousBrowserAction::ActionCache { command, .. } = action {
        return !matches!(command.as_str(), "stats" | "list" | "get");
    }
    matches!(
        action,
        AutonomousBrowserAction::Launch { .. }
            | AutonomousBrowserAction::Attach { .. }
            | AutonomousBrowserAction::UploadFile { .. }
            | AutonomousBrowserAction::Paste { .. }
            | AutonomousBrowserAction::DownloadSave { .. }
            | AutonomousBrowserAction::TraceStart { .. }
            | AutonomousBrowserAction::TraceStop { .. }
            | AutonomousBrowserAction::TraceExport { .. }
            | AutonomousBrowserAction::VisualBaselineSave { .. }
            | AutonomousBrowserAction::VisualDiff { .. }
            | AutonomousBrowserAction::HarExport { .. }
            | AutonomousBrowserAction::PdfExport { .. }
            | AutonomousBrowserAction::DebugBundle { .. }
            | AutonomousBrowserAction::ExportBundle { .. }
    ) || matches!(
        action,
        AutonomousBrowserAction::Recording { command, .. } if command == "export"
    ) || matches!(
        action,
        AutonomousBrowserAction::NetworkControl { .. }
            | AutonomousBrowserAction::StateRestore { .. }
            | AutonomousBrowserAction::VaultSave { .. }
            | AutonomousBrowserAction::VaultLogin { .. }
            | AutonomousBrowserAction::VaultDelete { .. }
            | AutonomousBrowserAction::AuthProfileSave { .. }
            | AutonomousBrowserAction::AuthProfileRestore { .. }
            | AutonomousBrowserAction::AuthProfileDelete { .. }
            | AutonomousBrowserAction::McpBridge { .. }
            | AutonomousBrowserAction::GenerateTest { .. }
    )
}

fn browser_action_risk_class(action: &AutonomousBrowserAction) -> &'static str {
    match action {
        AutonomousBrowserAction::InAppCdpFacade { method, .. } => {
            if in_app_cdp_facade_method_is_observe(method) {
                "browser_observe"
            } else {
                "browser_in_app_facade_control"
            }
        }
        AutonomousBrowserAction::ActionCache { command, .. } => {
            if matches!(command.as_str(), "stats" | "list" | "get") {
                "browser_observe"
            } else {
                "browser_action_cache_mutation"
            }
        }
        AutonomousBrowserAction::Attach {
            allow_remote_endpoint: Some(true),
            ..
        } => "browser_remote_cdp_control_channel",
        AutonomousBrowserAction::Launch { .. } | AutonomousBrowserAction::Attach { .. } => {
            "browser_cdp_control_channel"
        }
        AutonomousBrowserAction::UploadFile { .. }
        | AutonomousBrowserAction::DownloadSave { .. } => "browser_file_transfer",
        AutonomousBrowserAction::VaultSave { .. }
        | AutonomousBrowserAction::VaultLogin { .. }
        | AutonomousBrowserAction::VaultDelete { .. }
        | AutonomousBrowserAction::AuthProfileSave { .. }
        | AutonomousBrowserAction::AuthProfileRestore { .. }
        | AutonomousBrowserAction::AuthProfileDelete { .. }
        | AutonomousBrowserAction::StateRestore { .. } => "browser_credential_state",
        AutonomousBrowserAction::NetworkControl { .. } => "browser_network_interception",
        AutonomousBrowserAction::TraceStart { .. }
        | AutonomousBrowserAction::TraceStop { .. }
        | AutonomousBrowserAction::TraceExport { .. }
        | AutonomousBrowserAction::VisualBaselineSave { .. }
        | AutonomousBrowserAction::VisualDiff { .. }
        | AutonomousBrowserAction::HarExport { .. }
        | AutonomousBrowserAction::PdfExport { .. }
        | AutonomousBrowserAction::DebugBundle { .. }
        | AutonomousBrowserAction::ExportBundle { .. }
        | AutonomousBrowserAction::GenerateTest { .. } => "browser_evidence_persistence",
        AutonomousBrowserAction::McpBridge { .. } => "browser_external_bridge",
        _ => "browser_control",
    }
}

fn in_app_cdp_facade_method_is_observe(method: &str) -> bool {
    matches!(
        method,
        "Page.lifecycle"
            | "DOM.snapshot"
            | "DOM.resolveRef"
            | "Log.entryAdded"
            | "Network.requestWillBeSent"
            | "Network.responseReceived"
            | "Network.summary"
            | "Accessibility.snapshot"
            | "Storage.get"
    )
}

fn mcp_action_is_observe(action: AutonomousMcpAction) -> bool {
    matches!(
        action,
        AutonomousMcpAction::ListServers
            | AutonomousMcpAction::ListTools
            | AutonomousMcpAction::ListResources
            | AutonomousMcpAction::ListPrompts
            | AutonomousMcpAction::ReadResource
            | AutonomousMcpAction::GetPrompt
    )
}

fn command_family_policy_decision(
    runtime: &AutonomousToolRuntime,
    tool_name: &str,
    request: &AutonomousToolRequest,
) -> CommandResult<Option<(AutonomousSafetyPolicyAction, String, String)>> {
    if tool_name == AUTONOMOUS_TOOL_HOST_COMMAND {
        let AutonomousToolRequest::HostCommand(request) = request else {
            return Ok(None);
        };
        let mode = runtime.owner_admin_mode_status();
        if !mode.active {
            return Ok(Some((
                AutonomousSafetyPolicyAction::Deny,
                "policy_denied_owner_admin_mode_inactive".into(),
                format!(
                    "Xero denied host_command because local Owner Admin mode is not active: {}",
                    mode.reason
                ),
            )));
        }
        let policy = runtime.host_command_policy_trace(request, &mode)?;
        let action = if policy.outcome == AutonomousCommandPolicyOutcome::Allowed {
            AutonomousSafetyPolicyAction::Allow
        } else {
            AutonomousSafetyPolicyAction::RequireApproval
        };
        return Ok(Some((action, policy.code, policy.reason)));
    }

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
    Ok(Some(
        match runtime.evaluate_command_policy_for_tool(tool_name, prepared)? {
            CommandPolicyDecision::Allow { prepared, policy } => {
                if let Some(policy) = command_tool_scope_escalation(tool_name, &prepared, &policy) {
                    (
                        AutonomousSafetyPolicyAction::RequireApproval,
                        policy.code,
                        policy.reason,
                    )
                } else {
                    (
                        AutonomousSafetyPolicyAction::Allow,
                        policy.code,
                        policy.reason,
                    )
                }
            }
            CommandPolicyDecision::Escalate { policy, .. } => (
                AutonomousSafetyPolicyAction::RequireApproval,
                policy.code,
                policy.reason,
            ),
        },
    ))
}

fn command_tool_can_run_without_operator_review(
    tool_name: &str,
    prepared: &PreparedCommandRequest,
    policy: &AutonomousCommandPolicyTrace,
) -> bool {
    matches!(tool_name, AUTONOMOUS_TOOL_COMMAND_PROBE) && command_probe_allows(prepared, policy)
}

pub(super) fn command_tool_scope_escalation(
    tool_name: &str,
    prepared: &PreparedCommandRequest,
    policy: &AutonomousCommandPolicyTrace,
) -> Option<AutonomousCommandPolicyTrace> {
    if policy.approval_mode == RuntimeRunApprovalModeDto::Yolo {
        return None;
    }

    match tool_name {
        AUTONOMOUS_TOOL_COMMAND_PROBE if !command_probe_allows(prepared, policy) => {
            Some(policy_trace(
                AutonomousCommandPolicyOutcome::Escalated,
                policy.approval_mode.clone(),
                policy.profile,
                "policy_escalated_command_probe_scope",
                format!(
                    "Xero requires operator review for command_probe `{}` because probes are limited to read-only repository discovery commands.",
                    render_command_for_persistence(&prepared.argv)
                ),
            ))
        }
        AUTONOMOUS_TOOL_COMMAND_VERIFY if !command_verify_allows(prepared, policy) => {
            Some(policy_trace(
                AutonomousCommandPolicyOutcome::Escalated,
                policy.approval_mode.clone(),
                policy.profile,
                "policy_escalated_command_verify_scope",
                format!(
                    "Xero requires operator review for command_verify `{}` because verification is limited to known test, lint, typecheck, type-check, build, format, and check commands.",
                    render_command_for_persistence(&prepared.argv)
                ),
            ))
        }
        _ => None,
    }
}

fn command_probe_allows(
    prepared: &PreparedCommandRequest,
    policy: &AutonomousCommandPolicyTrace,
) -> bool {
    if policy.profile != AutonomousCommandPolicyProfile::ReadOnlyVerification {
        return false;
    }
    let program = executable_name(&prepared.argv[0]);
    match program {
        "pwd" | "ls" | "dir" | "echo" | "cat" | "type" | "head" | "tail" | "grep" | "rg" => true,
        "find" => !prepared.argv.iter().any(|argument| argument == "-delete"),
        "git" => git_subcommand(&prepared.argv).is_some_and(|subcommand| {
            matches!(
                subcommand,
                "status" | "diff" | "log" | "show" | "rev-parse" | "grep" | "ls-files"
            )
        }),
        "cargo" => {
            git_subcommand(&prepared.argv)
                .is_some_and(|subcommand| matches!(subcommand, "metadata" | "tree"))
                || version_probe_allows(&prepared.argv)
        }
        "node" | "npm" | "npx" | "pnpm" | "yarn" | "bun" | "deno" | "python" | "python3"
        | "rustc" | "go" | "tsc" | "vite" => version_probe_allows(&prepared.argv),
        _ => false,
    }
}

fn command_verify_allows(
    prepared: &PreparedCommandRequest,
    policy: &AutonomousCommandPolicyTrace,
) -> bool {
    if !matches!(
        policy.profile,
        AutonomousCommandPolicyProfile::ReadOnlyVerification
            | AutonomousCommandPolicyProfile::GeneratedFileMutation
    ) {
        return false;
    }
    let program = executable_name(&prepared.argv[0]);
    match program {
        "cargo" => git_subcommand(&prepared.argv).is_some_and(|subcommand| {
            matches!(
                subcommand,
                "build" | "check" | "clippy" | "doc" | "fmt" | "test"
            )
        }),
        "npm" | "pnpm" | "yarn" | "bun" => {
            package_manager_subcommand(&prepared.argv).is_some_and(|subcommand| {
                matches!(
                    subcommand,
                    "test"
                        | "tests"
                        | "lint"
                        | "typecheck"
                        | "type-check"
                        | "check"
                        | "build"
                        | "run"
                        | "run-script"
                )
            })
        }
        _ => false,
    }
}

fn version_probe_allows(argv: &[String]) -> bool {
    argv.len() == 2 && matches!(argv[1].as_str(), "--version" | "-v" | "-V" | "version")
}

fn git_subcommand(argv: &[String]) -> Option<&str> {
    argv.iter()
        .skip(1)
        .find(|argument| !argument.starts_with('-'))
        .map(String::as_str)
}

fn package_manager_subcommand(argv: &[String]) -> Option<&str> {
    let mut skip_next = false;
    for argument in argv.iter().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        let argument = argument.as_str();
        if argument == "--" {
            continue;
        }
        if package_manager_flag_takes_value(argument) {
            skip_next = true;
            continue;
        }
        if package_manager_flag_with_inline_value(argument) || argument.starts_with('-') {
            continue;
        }
        return Some(argument);
    }
    None
}

fn package_manager_flag_takes_value(argument: &str) -> bool {
    matches!(
        argument,
        "--filter" | "-F" | "--workspace" | "-w" | "--prefix" | "-C" | "--dir"
    )
}

fn package_manager_flag_with_inline_value(argument: &str) -> bool {
    matches!(
        argument.split_once('=').map(|(name, _)| name),
        Some("--filter" | "--workspace" | "--prefix" | "--dir")
    )
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

fn request_allows_linked_absolute_paths(request: &AutonomousToolRequest) -> bool {
    matches!(
        request,
        AutonomousToolRequest::Read(request) if !request.system_path
    ) || matches!(
        request,
        AutonomousToolRequest::ReadMany(_)
            | AutonomousToolRequest::Stat(_)
            | AutonomousToolRequest::Search(_)
            | AutonomousToolRequest::Find(_)
            | AutonomousToolRequest::List(_)
            | AutonomousToolRequest::ListTree(_)
    )
}

fn should_treat_policy_path_as_absolute(value: &str) -> bool {
    Path::new(value).is_absolute() || value == "~" || value.starts_with("~/")
}

fn repo_relative_paths(request: &AutonomousToolRequest) -> Vec<&str> {
    match request {
        AutonomousToolRequest::Read(request) if !request.system_path => vec![request.path.as_str()],
        AutonomousToolRequest::ReadMany(request) => {
            request.paths.iter().map(String::as_str).collect()
        }
        AutonomousToolRequest::Stat(request) => vec![request.path.as_str()],
        AutonomousToolRequest::Search(request) => {
            optional_repo_relative_path(request.path.as_deref())
        }
        AutonomousToolRequest::Find(request) => {
            optional_repo_relative_path(request.path.as_deref())
        }
        AutonomousToolRequest::List(request) => {
            optional_repo_relative_path(request.path.as_deref())
        }
        AutonomousToolRequest::ListTree(request) => {
            optional_repo_relative_path(request.path.as_deref())
        }
        AutonomousToolRequest::DirectoryDigest(request) => vec![request.path.as_str()],
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
        AutonomousToolRequest::Copy(request) => vec![request.from.as_str(), request.to.as_str()],
        AutonomousToolRequest::FsTransaction(request) => request
            .operations
            .iter()
            .flat_map(|operation| {
                [
                    operation.path.as_deref(),
                    operation.from.as_deref(),
                    operation.to.as_deref(),
                    operation.from_path.as_deref(),
                    operation.to_path.as_deref(),
                ]
                .into_iter()
                .flatten()
            })
            .collect(),
        AutonomousToolRequest::JsonEdit(request)
        | AutonomousToolRequest::TomlEdit(request)
        | AutonomousToolRequest::YamlEdit(request) => vec![request.path.as_str()],
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
        .filter(|path| !path.is_empty() && !is_current_directory_path(path))
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

pub(super) fn system_diagnostics_policy_trace(
    action: AutonomousSystemDiagnosticsAction,
) -> AutonomousSystemDiagnosticsPolicyTrace {
    let risk_level = match action {
        AutonomousSystemDiagnosticsAction::ProcessOpenFiles
        | AutonomousSystemDiagnosticsAction::ProcessResourceSnapshot
        | AutonomousSystemDiagnosticsAction::ProcessThreads
        | AutonomousSystemDiagnosticsAction::SystemLogQuery
        | AutonomousSystemDiagnosticsAction::DiagnosticsBundle => {
            AutonomousProcessActionRiskLevel::SystemRead
        }
        AutonomousSystemDiagnosticsAction::ProcessSample => {
            AutonomousProcessActionRiskLevel::SystemRead
        }
        AutonomousSystemDiagnosticsAction::MacosAccessibilitySnapshot => {
            AutonomousProcessActionRiskLevel::OsAutomation
        }
    };

    let (approval_required, code, reason) = match action {
        AutonomousSystemDiagnosticsAction::ProcessSample => (
            true,
            "system_diagnostics_process_sample_requires_approval",
            "Process sampling can capture sensitive runtime details and requires operator approval.",
        ),
        AutonomousSystemDiagnosticsAction::MacosAccessibilitySnapshot => (
            true,
            "system_diagnostics_accessibility_snapshot_requires_approval",
            "macOS Accessibility snapshots can expose screen and UI state and require operator approval.",
        ),
        AutonomousSystemDiagnosticsAction::ProcessOpenFiles => (
            false,
            "system_diagnostics_process_open_files_allowed",
            "Bounded process open-file inspection is read-only system observation.",
        ),
        AutonomousSystemDiagnosticsAction::ProcessResourceSnapshot => (
            false,
            "system_diagnostics_resource_snapshot_allowed",
            "Bounded process resource snapshots are read-only system observation.",
        ),
        AutonomousSystemDiagnosticsAction::ProcessThreads => (
            false,
            "system_diagnostics_process_threads_allowed",
            "Bounded process thread inspection is read-only system observation.",
        ),
        AutonomousSystemDiagnosticsAction::SystemLogQuery => (
            false,
            "system_diagnostics_log_query_allowed",
            "Bounded filtered system log queries are read-only system observation.",
        ),
        AutonomousSystemDiagnosticsAction::DiagnosticsBundle => (
            false,
            "system_diagnostics_bundle_allowed",
            "Diagnostics bundles compose bounded typed diagnostics and stop at approval boundaries.",
        ),
    };

    AutonomousSystemDiagnosticsPolicyTrace {
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

fn normalize_command_cwd(value: &str) -> CommandResult<Option<PathBuf>> {
    validate_non_empty(value, "cwd")?;
    let trimmed = value.trim();
    if is_current_directory_path(trimmed) {
        return Ok(None);
    }
    normalize_relative_path(trimmed, "cwd")
        .map(Some)
        .map_err(map_cwd_policy_error)
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

impl AutonomousToolRuntime {
    fn validate_repo_scoped_arguments(
        &self,
        prepared: &PreparedCommandRequest,
        approval_mode: RuntimeRunApprovalModeDto,
    ) -> CommandResult<()> {
        for argument in prepared.argv.iter().skip(1) {
            let Some(candidate) = extract_path_candidate(argument) else {
                continue;
            };

            self.validate_command_argument_path(candidate)
                .map_err(|error| {
                    if error.class == CommandErrorClass::PolicyDenied {
                        CommandError::new(
                            "policy_denied_argument_outside_repo",
                            CommandErrorClass::PolicyDenied,
                            format!(
                                "Xero denied the autonomous shell command under active approval mode `{}` because argument `{candidate}` escapes the imported repository root and linked context paths.",
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

    fn validate_command_argument_path(&self, candidate: &str) -> CommandResult<()> {
        if is_current_directory_path(candidate.trim()) {
            return Ok(());
        }
        let candidate_path = Path::new(candidate);
        if candidate_path.is_absolute() {
            return self.validate_absolute_command_argument_path(candidate_path);
        }

        normalize_relative_path(candidate, "argv").map(|_| ())
    }

    fn validate_absolute_command_argument_path(&self, candidate: &Path) -> CommandResult<()> {
        let resolved = fs::canonicalize(candidate).map_err(|_| {
            CommandError::new(
                "autonomous_tool_path_denied",
                CommandErrorClass::PolicyDenied,
                format!(
                    "Xero denied `{}` because autonomous command arguments must resolve inside the imported repository root or a linked context path.",
                    candidate.display()
                ),
                false,
            )
        })?;

        if resolved == self.repo_root || resolved.starts_with(&self.repo_root) {
            return Ok(());
        }

        if self.linked_read_roots.iter().any(|root| {
            if root.is_dir {
                resolved == root.path || resolved.starts_with(&root.path)
            } else {
                resolved == root.path
            }
        }) {
            return Ok(());
        }

        Err(CommandError::new(
            "autonomous_tool_path_denied",
            CommandErrorClass::PolicyDenied,
            format!(
                "Xero denied `{}` because it resolves outside the imported repository root and linked context paths.",
                candidate.display()
            ),
            false,
        ))
    }
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
    Safe {
        profile: AutonomousCommandPolicyProfile,
        reason: String,
    },
    Escalated {
        profile: AutonomousCommandPolicyProfile,
        code: &'static str,
        reason: String,
    },
}

fn classify_command(prepared: &PreparedCommandRequest) -> CommandClassification {
    let argv = &prepared.argv;
    let program = executable_name(&argv[0]);

    if version_probe_allows(argv) {
        match program {
            "node" | "npm" | "npx" | "pnpm" | "yarn" | "bun" | "deno" | "python" | "python3"
            | "cargo" | "rustc" | "go" | "tsc" | "vite" => {
                return safe_command(argv);
            }
            _ => {}
        }
    }

    if is_shell_wrapper(program) {
        if shell_wrapper_contains_sensitive_pattern(argv) {
            return CommandClassification::Escalated {
                profile: AutonomousCommandPolicyProfile::ExternalNetwork,
                code: "policy_escalated_sensitive_shell",
                reason: format!(
                    "Xero requires operator review for shell wrapper command `{}` because the script may expand environment variables, access absolute paths, or contact external network surfaces.",
                    render_command_for_summary(argv)
                ),
            };
        }
        if shell_wrapper_contains_destructive_pattern(argv) {
            return CommandClassification::Escalated {
                profile: AutonomousCommandPolicyProfile::DestructiveOperation,
                code: "policy_escalated_destructive_shell",
                reason: format!(
                    "Xero requires operator review for shell wrapper command `{}` because the script text matches the destructive command classifier.",
                    render_command_for_summary(argv)
                ),
            };
        }
        return safe_command_with_profile(argv, AutonomousCommandPolicyProfile::GeneralExecution);
    }

    match program {
        "curl" | "wget" | "nc" | "netcat" | "ssh" | "scp" | "sftp" | "ftp" | "ping" => {
            CommandClassification::Escalated {
                profile: AutonomousCommandPolicyProfile::ExternalNetwork,
                code: "policy_escalated_network_command",
                reason: format!(
                    "Xero requires operator review for `{}` because it can contact external network surfaces.",
                    render_command_for_summary(argv)
                ),
            }
        }
        "openssl" if argv.iter().any(|argument| argument == "s_client") => {
            CommandClassification::Escalated {
                profile: AutonomousCommandPolicyProfile::ExternalNetwork,
                code: "policy_escalated_network_command",
                reason: format!(
                    "Xero requires operator review for `{}` because it can contact external network surfaces.",
                    render_command_for_summary(argv)
                ),
            }
        }
        "pwd" | "ls" | "dir" | "echo" | "cat" | "type" | "head" | "tail" | "grep" | "rg"
        | "sleep" => safe_command(argv),
        "find" => {
            if argv.iter().any(|argument| argument == "-delete") {
                return CommandClassification::Escalated {
                    profile: AutonomousCommandPolicyProfile::DestructiveOperation,
                    code: "policy_escalated_destructive_command",
                    reason: format!(
                        "Xero requires operator review for `{}` because `find -delete` is destructive.",
                        render_command_for_summary(argv)
                    ),
                };
            }
            safe_command(argv)
        }
        "git" => classify_git_command(argv),
        "cargo" => classify_cargo_command(argv),
        "npm" | "pnpm" | "yarn" | "bun" => classify_package_manager_command(argv, &prepared.cwd),
        "rm" | "rmdir" | "del" | "erase" | "rd" | "mv" | "move" | "chmod" | "chown" | "dd"
        | "mkfs" | "diskutil" => CommandClassification::Escalated {
            profile: AutonomousCommandPolicyProfile::DestructiveOperation,
            code: "policy_escalated_destructive_command",
            reason: format!(
                "Xero requires operator review for `{}` because it matches the destructive command classifier.",
                render_command_for_summary(argv)
            ),
        },
        _ => CommandClassification::Escalated {
            profile: AutonomousCommandPolicyProfile::GeneralExecution,
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
            if argv
                .iter()
                .any(|argument| matches!(argument.as_str(), "-d" | "-D" | "--delete"))
            {
                destructive_command(argv, "git branch delete flags are destructive")
            } else {
                safe_command(argv)
            }
        }
        Some("tag") => {
            if argv
                .iter()
                .any(|argument| matches!(argument.as_str(), "-d" | "--delete"))
            {
                destructive_command(argv, "git tag delete flags are destructive")
            } else {
                safe_command(argv)
            }
        }
        Some("add" | "commit" | "mv" | "rm") => safe_command(argv),
        Some(
            "clean" | "reset" | "checkout" | "switch" | "restore" | "stash" | "merge" | "rebase"
            | "cherry-pick" | "revert" | "push" | "pull",
        ) => destructive_command(argv, "the git subcommand mutates repository state"),
        Some(_) | None => CommandClassification::Escalated {
            profile: AutonomousCommandPolicyProfile::GeneralExecution,
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
        Some("build" | "doc" | "fmt") => {
            safe_command_with_profile(argv, AutonomousCommandPolicyProfile::GeneratedFileMutation)
        }
        Some("check" | "clippy" | "metadata" | "test" | "tree") => safe_command(argv),
        Some(_) | None => CommandClassification::Escalated {
            profile: AutonomousCommandPolicyProfile::GeneralExecution,
            code: "policy_escalated_ambiguous_command",
            reason: format!(
                "Xero could not classify cargo command `{}` as non-destructive, so operator review is required.",
                render_command_for_summary(argv)
            ),
        },
    }
}

fn classify_package_manager_command(argv: &[String], cwd: &Path) -> CommandClassification {
    match package_manager_subcommand(argv) {
        Some("install" | "add" | "remove" | "unlink" | "upgrade" | "update") => {
            CommandClassification::Escalated {
                profile: AutonomousCommandPolicyProfile::DependencyInstallation,
                code: "policy_escalated_package_manager_mutation",
                reason: format!(
                    "Xero requires operator review for `{}` because package-manager mutation commands can execute install scripts, change dependency state, or contact external registries.",
                    render_command_for_summary(argv)
                ),
            }
        }
        Some(script @ ("test" | "lint" | "typecheck" | "type-check" | "build")) => {
            classify_repo_package_script(argv, cwd, script, true)
        }
        Some("exec") => CommandClassification::Escalated {
            profile: AutonomousCommandPolicyProfile::DependencyInstallation,
            code: "policy_escalated_package_manager_exec",
            reason: format!(
                "Xero requires operator review for `{}` because package-manager exec commands can run arbitrary local or registry-provided binaries.",
                render_command_for_summary(argv)
            ),
        },
        Some("publish") => destructive_command(
            argv,
            "package manager publish commands can affect external registries",
        ),
        Some("run" | "run-script") => classify_package_manager_run_script(argv, cwd),
        Some(_) | None => CommandClassification::Escalated {
            profile: AutonomousCommandPolicyProfile::GeneralExecution,
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
        return CommandClassification::Escalated {
            profile: AutonomousCommandPolicyProfile::GeneralExecution,
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
        return CommandClassification::Escalated {
            profile: AutonomousCommandPolicyProfile::GeneralExecution,
            code: "policy_escalated_package_manager_run",
            reason: format!(
                "Xero requires operator review for `{}` because package script `{script_name}` is not in the repo-local verification allowlist.",
                render_command_for_summary(argv)
            ),
        };
    }

    let Some(script) = package_json_script(cwd, script_name) else {
        if direct_script_command {
            return safe_package_script_command(argv, script_name, false);
        }
        return CommandClassification::Escalated {
            profile: AutonomousCommandPolicyProfile::GeneralExecution,
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
        return CommandClassification::Escalated {
            profile: AutonomousCommandPolicyProfile::DestructiveOperation,
            code: "policy_escalated_destructive_package_script",
            reason: format!(
                "Xero requires operator review for `{}` because package script `{script_name}` contains destructive shell patterns.",
                render_command_for_summary(argv)
            ),
        };
    }
    if shell_wrapper_contains_sensitive_pattern(&shell_argv) {
        return CommandClassification::Escalated {
            profile: AutonomousCommandPolicyProfile::ExternalNetwork,
            code: "policy_escalated_sensitive_package_script",
            reason: format!(
                "Xero requires operator review for `{}` because package script `{script_name}` may expand secrets, access absolute paths, or contact external network surfaces.",
                render_command_for_summary(argv)
            ),
        };
    }

    CommandClassification::Safe {
        profile: package_script_profile(script_name),
        reason: format!(
            "Active approval mode `yolo` allowed repo-local package script `{script_name}` via `{}` after package.json introspection classified the script as verification-safe.",
            render_command_for_summary(argv)
        ),
    }
}

fn is_safe_package_script_name(script_name: &str) -> bool {
    matches!(
        script_name,
        "test" | "tests" | "lint" | "typecheck" | "type-check" | "check" | "build" | "rust:test"
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
    safe_command_with_profile(argv, AutonomousCommandPolicyProfile::ReadOnlyVerification)
}

fn safe_command_with_profile(
    argv: &[String],
    profile: AutonomousCommandPolicyProfile,
) -> CommandClassification {
    CommandClassification::Safe {
        profile,
        reason: format!(
            "Active approval mode `yolo` allowed repo-scoped command `{}` because it matched the non-destructive command classifier.",
            render_command_for_summary(argv)
        ),
    }
}

fn safe_package_script_command(
    argv: &[String],
    script_name: &str,
    introspected: bool,
) -> CommandClassification {
    let profile = package_script_profile(script_name);
    let reason = if introspected {
        format!(
            "Active approval mode `yolo` allowed repo-local package script `{script_name}` via `{}` after package.json introspection classified the script as verification-safe.",
            render_command_for_summary(argv)
        )
    } else {
        format!(
            "Active approval mode `yolo` allowed package-manager verification command `{}` because script `{script_name}` is in the safe script allowlist.",
            render_command_for_summary(argv)
        )
    };
    CommandClassification::Safe { profile, reason }
}

fn package_script_profile(script_name: &str) -> AutonomousCommandPolicyProfile {
    if script_name == "build" {
        AutonomousCommandPolicyProfile::GeneratedFileMutation
    } else {
        AutonomousCommandPolicyProfile::ReadOnlyVerification
    }
}

fn destructive_command(argv: &[String], reason: &str) -> CommandClassification {
    CommandClassification::Escalated {
        profile: AutonomousCommandPolicyProfile::DestructiveOperation,
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
    profile: AutonomousCommandPolicyProfile,
    code: impl Into<String>,
    reason: impl Into<String>,
) -> AutonomousCommandPolicyTrace {
    AutonomousCommandPolicyTrace {
        outcome,
        approval_mode,
        profile,
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
    use crate::runtime::AutonomousToolOutput;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn mailbox_retry_guidance_recommends_path_scoped_read_when_paths_are_known() {
        let guidance = mailbox_check_retry_guidance(&["src/lib.rs".into()]);

        assert!(guidance.contains("check_inbox_status"));
        assert!(guidance.contains("read_inbox"));
        assert!(guidance.contains(r#""src/lib.rs""#));
    }

    #[test]
    fn native_browser_gap_actions_have_observe_control_and_approval_policy() {
        let observe_actions = [
            AutonomousBrowserAction::DialogList { session_id: None },
            AutonomousBrowserAction::DownloadList { session_id: None },
            AutonomousBrowserAction::TraceStatus { session_id: None },
            AutonomousBrowserAction::VisualBaselineList { session_id: None },
            AutonomousBrowserAction::EmulationState { session_id: None },
            AutonomousBrowserAction::Extract {
                session_id: None,
                mode: "page_summary".into(),
                selector: None,
                selector_map: None,
                limit: None,
            },
            AutonomousBrowserAction::BrowserResource {
                session_id: None,
                resource: "current_state".into(),
            },
        ];
        for action in observe_actions {
            assert!(
                browser_action_is_observe(&action),
                "{action:?} should be observe"
            );
            assert!(!browser_action_requires_approval(&action));
        }

        for (action, risk_class) in [
            (
                AutonomousBrowserAction::Launch {
                    session_id: None,
                    label: None,
                    url: None,
                    browser_path: None,
                    headless: None,
                    sensitive_mode: None,
                },
                "browser_cdp_control_channel",
            ),
            (
                AutonomousBrowserAction::Attach {
                    endpoint: "http://127.0.0.1:9222".into(),
                    session_id: None,
                    label: None,
                    sensitive_mode: None,
                    allow_remote_endpoint: None,
                },
                "browser_cdp_control_channel",
            ),
            (
                AutonomousBrowserAction::UploadFile {
                    selector: Some("input[type=file]".into()),
                    ref_id: None,
                    paths: vec!["/tmp/file.txt".into()],
                    timeout_ms: None,
                },
                "browser_file_transfer",
            ),
            (
                AutonomousBrowserAction::DownloadSave {
                    session_id: None,
                    guid: "download-1".into(),
                    destination: "/tmp/download.txt".into(),
                },
                "browser_file_transfer",
            ),
            (
                AutonomousBrowserAction::AuthProfileRestore {
                    session_id: None,
                    name: "fixture".into(),
                    navigate: Some(true),
                },
                "browser_credential_state",
            ),
            (
                AutonomousBrowserAction::TraceExport { session_id: None },
                "browser_evidence_persistence",
            ),
            (
                AutonomousBrowserAction::McpBridge {
                    command: "status".into(),
                },
                "browser_external_bridge",
            ),
        ] {
            assert!(
                !browser_action_is_observe(&action),
                "{action:?} should be control"
            );
            assert!(browser_action_requires_approval(&action));
            assert_eq!(browser_action_risk_class(&action), risk_class);
        }
    }

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
            CommandClassification::Safe { profile, reason } => {
                assert_eq!(
                    profile,
                    AutonomousCommandPolicyProfile::ReadOnlyVerification
                );
                assert!(reason.contains("package.json introspection"));
                assert!(reason.contains("test"));
            }
            other => panic!("expected safe script, got {other:?}"),
        }
    }

    #[test]
    fn package_manager_type_check_forms_are_verification_safe() {
        let tempdir = tempdir().expect("tempdir");
        fs::write(
            tempdir.path().join("package.json"),
            r#"{"scripts":{"type-check":"tsc --noEmit","typecheck":"tsc --noEmit"}}"#,
        )
        .expect("package");

        for argv in [
            ["pnpm", "type-check"].as_slice(),
            ["pnpm", "--filter", "client", "type-check"].as_slice(),
            ["pnpm", "run", "type-check"].as_slice(),
        ] {
            let prepared = PreparedCommandRequest {
                argv: argv.iter().map(|value| (*value).to_owned()).collect(),
                cwd_relative: None,
                cwd: tempdir.path().to_path_buf(),
                timeout_ms: DEFAULT_COMMAND_TIMEOUT_MS,
            };
            let decision = classify_command(&prepared);

            match decision {
                CommandClassification::Safe { profile, reason } => {
                    assert_eq!(
                        profile,
                        AutonomousCommandPolicyProfile::ReadOnlyVerification
                    );
                    assert!(reason.contains("type-check"));
                }
                other => panic!("expected safe type-check script, got {other:?}"),
            }
        }
    }

    #[test]
    fn package_manager_type_check_variants_stay_reviewed() {
        let tempdir = tempdir().expect("tempdir");
        fs::write(
            tempdir.path().join("package.json"),
            r#"{"scripts":{"type-check:ci":"tsc --noEmit"}}"#,
        )
        .expect("package");
        let prepared = prepared_command(tempdir.path(), ["pnpm", "run", "type-check:ci"]);

        let decision = classify_command(&prepared);

        match decision {
            CommandClassification::Escalated { code, reason, .. } => {
                assert_eq!(code, "policy_escalated_package_manager_run");
                assert!(reason.contains("verification allowlist"));
            }
            other => panic!("expected reviewed type-check variant, got {other:?}"),
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
            CommandClassification::Escalated {
                profile,
                code,
                reason,
            } => {
                assert_eq!(
                    profile,
                    AutonomousCommandPolicyProfile::DestructiveOperation
                );
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
            CommandClassification::Escalated {
                profile,
                code,
                reason,
            } => {
                assert_eq!(profile, AutonomousCommandPolicyProfile::GeneralExecution);
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
    fn safety_policy_allows_oauth_invite_url_in_write_content() {
        let tempdir = tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path(), RuntimeRunApprovalModeDto::Yolo);
        let content = "window.location.href = 'https://discord.com/oauth2/authorize?client_id=123456789012345678&permissions=8&scope=bot%20applications.commands';";
        let request = AutonomousToolRequest::Write(super::super::AutonomousWriteRequest {
            path: "index.html".into(),
            content: content.into(),
            expected_hash: None,
            create_only: false,
            overwrite: None,
            preview: false,
        });

        let decision = runtime
            .evaluate_safety_policy(
                "write",
                &json!({"path": "index.html", "content": content}),
                &request,
                false,
                "input-hash",
            )
            .expect("policy");

        assert_eq!(decision.action, AutonomousSafetyPolicyAction::Allow);
        assert_eq!(decision.code, "policy_allowed_tool_call");
    }

    #[test]
    fn safety_policy_allows_fs_transaction_with_css_mask_image_content() {
        let tempdir = tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path(), RuntimeRunApprovalModeDto::Yolo);
        let content = ".grain::before { -webkit-mask-image: radial-gradient(circle, black, transparent); mask-image: linear-gradient(black, transparent); }";
        let request =
            AutonomousToolRequest::FsTransaction(super::super::AutonomousFsTransactionRequest {
                operations: vec![super::super::AutonomousFsTransactionOperation {
                    action: AutonomousFsTransactionAction::CreateFile,
                    path: Some("src/index.css".into()),
                    content: Some(content.into()),
                    ..super::super::AutonomousFsTransactionOperation::default()
                }],
                preview: false,
                stop_on_first_error: true,
            });

        let decision = runtime
            .evaluate_safety_policy(
                super::super::AUTONOMOUS_TOOL_FS_TRANSACTION,
                &json!({
                    "operations": [
                        {
                            "action": "create_file",
                            "path": "src/index.css",
                            "content": content,
                        }
                    ],
                    "preview": false,
                }),
                &request,
                false,
                "input-hash",
            )
            .expect("policy");

        assert_eq!(decision.action, AutonomousSafetyPolicyAction::Allow);
        assert_eq!(decision.code, "policy_allowed_tool_call");
    }

    #[test]
    fn safety_policy_denies_repo_path_escape() {
        let tempdir = tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path(), RuntimeRunApprovalModeDto::Yolo);
        let request = AutonomousToolRequest::Write(super::super::AutonomousWriteRequest {
            path: "../outside.txt".into(),
            content: "hello".into(),
            expected_hash: None,
            create_only: false,
            overwrite: None,
            preview: false,
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
    fn safety_policy_allows_linked_context_system_read_without_approval() {
        let repo = tempdir().expect("repo");
        let linked = tempdir().expect("linked");
        let linked_file = linked.path().join("notes.txt");
        fs::write(&linked_file, "linked context\n").expect("linked file");
        let unlinked = tempdir().expect("unlinked");
        let unlinked_file = unlinked.path().join("notes.txt");
        fs::write(&unlinked_file, "unlinked context\n").expect("unlinked file");
        let runtime = test_runtime(repo.path(), RuntimeRunApprovalModeDto::Yolo)
            .with_linked_read_roots(vec![linked.path().to_path_buf()])
            .expect("linked roots");

        let linked_request = AutonomousToolRequest::Read(super::super::AutonomousReadRequest {
            path: linked_file.display().to_string(),
            system_path: true,
            mode: Some(super::super::AutonomousReadMode::Text),
            start_line: None,
            line_count: None,
            cursor: None,
            around_pattern: None,
            max_bytes_per_file: None,
            byte_offset: None,
            byte_count: None,
            include_line_hashes: false,
        });
        let linked_decision = runtime
            .evaluate_safety_policy(
                super::super::AUTONOMOUS_TOOL_READ,
                &json!({"path": linked_file.display().to_string(), "systemPath": true}),
                &linked_request,
                false,
                "input-hash",
            )
            .expect("linked policy");
        assert_eq!(linked_decision.action, AutonomousSafetyPolicyAction::Allow);

        let unlinked_request = AutonomousToolRequest::Read(super::super::AutonomousReadRequest {
            path: unlinked_file.display().to_string(),
            system_path: true,
            mode: Some(super::super::AutonomousReadMode::Text),
            start_line: None,
            line_count: None,
            cursor: None,
            around_pattern: None,
            max_bytes_per_file: None,
            byte_offset: None,
            byte_count: None,
            include_line_hashes: false,
        });
        let unlinked_decision = runtime
            .evaluate_safety_policy(
                super::super::AUTONOMOUS_TOOL_READ,
                &json!({"path": unlinked_file.display().to_string(), "systemPath": true}),
                &unlinked_request,
                false,
                "input-hash",
            )
            .expect("unlinked policy");
        assert_eq!(
            unlinked_decision.action,
            AutonomousSafetyPolicyAction::RequireApproval
        );
        assert_eq!(
            unlinked_decision.code,
            "policy_requires_approval_system_read"
        );
    }

    #[test]
    fn safety_policy_allows_linked_context_list_without_approval() {
        let repo = tempdir().expect("repo");
        let linked = tempdir().expect("linked");
        let unlinked = tempdir().expect("unlinked");
        let runtime = test_runtime(repo.path(), RuntimeRunApprovalModeDto::Yolo)
            .with_linked_read_roots(vec![linked.path().to_path_buf()])
            .expect("linked roots");

        let linked_request = AutonomousToolRequest::List(super::super::AutonomousListRequest {
            path: Some(linked.path().display().to_string()),
            max_depth: Some(2),
            max_results: None,
            sort_by: None,
            sort_direction: None,
            cursor: None,
        });
        let linked_decision = runtime
            .evaluate_safety_policy(
                super::super::AUTONOMOUS_TOOL_LIST,
                &json!({"path": linked.path().display().to_string(), "maxDepth": 2}),
                &linked_request,
                false,
                "input-hash",
            )
            .expect("linked list policy");
        assert_eq!(linked_decision.action, AutonomousSafetyPolicyAction::Allow);

        let search_request = AutonomousToolRequest::Search(super::super::AutonomousSearchRequest {
            query: "Panda".into(),
            path: Some(linked.path().display().to_string()),
            regex: false,
            ignore_case: false,
            include_hidden: false,
            include_ignored: false,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            context_lines: None,
            max_results: None,
            files_only: false,
            cursor: None,
        });
        let search_decision = runtime
            .evaluate_safety_policy(
                super::super::AUTONOMOUS_TOOL_SEARCH,
                &json!({"query": "Panda", "path": linked.path().display().to_string()}),
                &search_request,
                false,
                "input-hash",
            )
            .expect("linked search policy");
        assert_eq!(search_decision.action, AutonomousSafetyPolicyAction::Allow);

        let find_request = AutonomousToolRequest::Find(super::super::AutonomousFindRequest {
            pattern: "**/*.ts".into(),
            mode: None,
            path: Some(linked.path().display().to_string()),
            max_depth: None,
            max_results: None,
            include_hidden: false,
            include_ignored: false,
            cursor: None,
        });
        let find_decision = runtime
            .evaluate_safety_policy(
                super::super::AUTONOMOUS_TOOL_FIND,
                &json!({"pattern": "**/*.ts", "path": linked.path().display().to_string()}),
                &find_request,
                false,
                "input-hash",
            )
            .expect("linked find policy");
        assert_eq!(find_decision.action, AutonomousSafetyPolicyAction::Allow);

        let unlinked_request = AutonomousToolRequest::List(super::super::AutonomousListRequest {
            path: Some(unlinked.path().display().to_string()),
            max_depth: Some(2),
            max_results: None,
            sort_by: None,
            sort_direction: None,
            cursor: None,
        });
        let unlinked_decision = runtime
            .evaluate_safety_policy(
                super::super::AUTONOMOUS_TOOL_LIST,
                &json!({"path": unlinked.path().display().to_string(), "maxDepth": 2}),
                &unlinked_request,
                false,
                "input-hash",
            )
            .expect("unlinked list policy");
        assert_eq!(unlinked_decision.action, AutonomousSafetyPolicyAction::Deny);
        assert_eq!(unlinked_decision.code, "policy_denied_path_escape");
    }

    #[test]
    fn safety_policy_allows_blank_optional_observe_scope_as_repo_root() {
        let tempdir = tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path(), RuntimeRunApprovalModeDto::Yolo);
        for path in ["", "."] {
            let request = AutonomousToolRequest::List(super::super::AutonomousListRequest {
                path: Some(path.into()),
                max_depth: Some(2),
                max_results: None,
                sort_by: None,
                sort_direction: None,
                cursor: None,
            });

            let decision = runtime
                .evaluate_safety_policy(
                    "list",
                    &json!({"path": path, "maxDepth": 2}),
                    &request,
                    false,
                    "input-hash",
                )
                .expect("policy");

            assert_eq!(decision.action, AutonomousSafetyPolicyAction::Allow);
        }
    }

    #[test]
    fn safety_policy_allows_computer_use_observation_without_approval() {
        let tempdir = tempdir().expect("tempdir");
        let runtime = test_runtime_for_agent(
            tempdir.path(),
            RuntimeRunApprovalModeDto::Yolo,
            RuntimeAgentIdDto::ComputerUse,
        );
        let requests = [
            (
                super::super::AUTONOMOUS_TOOL_MACOS_AUTOMATION,
                json!({"action": "mac_screenshot"}),
                AutonomousToolRequest::MacosAutomation(
                    super::super::AutonomousMacosAutomationRequest {
                        action: AutonomousMacosAutomationAction::MacScreenshot,
                        app_name: None,
                        bundle_id: None,
                        pid: None,
                        window_id: None,
                        monitor_id: None,
                        screenshot_target: None,
                    },
                ),
            ),
            (
                super::super::AUTONOMOUS_TOOL_DESKTOP_OBSERVE,
                json!({"action": "screenshot"}),
                AutonomousToolRequest::DesktopObserve(
                    super::super::AutonomousDesktopObserveRequest {
                        action: AutonomousDesktopObserveAction::Screenshot,
                        display_id: None,
                        window_id: None,
                        region: None,
                        x: None,
                        y: None,
                        include_data: None,
                        max_bytes: None,
                    },
                ),
            ),
            (
                super::super::AUTONOMOUS_TOOL_DESKTOP_STREAM,
                json!({"action": "stream_start"}),
                AutonomousToolRequest::DesktopStream(
                    super::super::AutonomousDesktopStreamRequest {
                        action: super::super::AutonomousDesktopStreamAction::StreamStart,
                        session_id: Some("session-1".into()),
                        run_id: Some("run-1".into()),
                        display_id: None,
                        stream_id: None,
                        max_width: None,
                        max_frame_rate: None,
                        include_cursor: None,
                        quality: None,
                        ice_servers: Vec::new(),
                        session_description: None,
                        ice_candidate: None,
                    },
                ),
            ),
        ];

        for (tool_name, raw, request) in requests {
            let decision = runtime
                .evaluate_safety_policy(tool_name, &raw, &request, false, "input-hash")
                .expect("policy");

            assert_eq!(decision.action, AutonomousSafetyPolicyAction::Allow);
        }
    }

    #[test]
    fn safety_policy_requires_approval_for_destructive_computer_use_actions() {
        let tempdir = tempdir().expect("tempdir");
        let runtime = test_runtime_for_agent(
            tempdir.path(),
            RuntimeRunApprovalModeDto::Yolo,
            RuntimeAgentIdDto::ComputerUse,
        );
        let requests = [
            (
                super::super::AUTONOMOUS_TOOL_MACOS_AUTOMATION,
                json!({"action": "mac_app_quit", "appName": "TextEdit"}),
                AutonomousToolRequest::MacosAutomation(
                    super::super::AutonomousMacosAutomationRequest {
                        action: AutonomousMacosAutomationAction::MacAppQuit,
                        app_name: Some("TextEdit".into()),
                        bundle_id: None,
                        pid: None,
                        window_id: None,
                        monitor_id: None,
                        screenshot_target: None,
                    },
                ),
            ),
            (
                super::super::AUTONOMOUS_TOOL_DESKTOP_CONTROL,
                json!({"action": "quit_app", "appName": "TextEdit"}),
                AutonomousToolRequest::DesktopControl(
                    super::super::AutonomousDesktopControlRequest {
                        action: AutonomousDesktopControlAction::QuitApp,
                        display_id: None,
                        window_id: None,
                        app_name: Some("TextEdit".into()),
                        bundle_id: None,
                        element_id: None,
                        x: None,
                        y: None,
                        source_width: None,
                        source_height: None,
                        to_x: None,
                        to_y: None,
                        delta_x: None,
                        delta_y: None,
                        width: None,
                        height: None,
                        include_data: None,
                        max_bytes: None,
                        media_type: None,
                        image_data_base64: None,
                        file_paths: Vec::new(),
                        button: None,
                        clicks: None,
                        key: None,
                        keys: Vec::new(),
                        text: None,
                        html: None,
                        rtf: None,
                        alt_text: None,
                        target_label: None,
                        selection_start: None,
                        selection_end: None,
                        value: None,
                        menu_path: Vec::new(),
                        reason: None,
                        sensitivity: None,
                    },
                ),
            ),
        ];

        for (tool_name, raw, request) in requests {
            let decision = runtime
                .evaluate_safety_policy(tool_name, &raw, &request, false, "input-hash")
                .expect("policy");

            assert_eq!(
                decision.action,
                AutonomousSafetyPolicyAction::RequireApproval
            );
        }
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

    #[test]
    fn safety_policy_allows_command_probe_under_linked_context_path() {
        let tempdir = tempdir().expect("tempdir");
        let repo_root = tempdir.path().join("repo");
        let linked_root = tempdir.path().join("linked-project");
        fs::create_dir_all(&repo_root).expect("repo root");
        fs::create_dir_all(&linked_root).expect("linked root");
        let runtime = test_runtime(&repo_root, RuntimeRunApprovalModeDto::Yolo)
            .with_linked_read_roots(vec![linked_root.clone()])
            .expect("linked roots");
        let request = AutonomousToolRequest::Command(AutonomousCommandRequest {
            argv: vec![
                "ls".into(),
                "-la".into(),
                linked_root.to_string_lossy().into_owned(),
            ],
            cwd: None,
            timeout_ms: Some(1_000),
        });

        let decision = runtime
            .evaluate_safety_policy(
                AUTONOMOUS_TOOL_COMMAND_PROBE,
                &json!({}),
                &request,
                false,
                "input",
            )
            .expect("decision");

        assert_eq!(decision.action, AutonomousSafetyPolicyAction::Allow);
        assert_eq!(decision.code, "policy_allowed_repo_scoped_command");
    }

    #[test]
    fn safety_policy_allows_command_probe_in_suggest_mode_for_linked_context_path() {
        let tempdir = tempdir().expect("tempdir");
        let repo_root = tempdir.path().join("repo");
        let linked_root = tempdir.path().join("linked-project");
        fs::create_dir_all(&repo_root).expect("repo root");
        fs::create_dir_all(&linked_root).expect("linked root");
        let runtime = test_runtime(&repo_root, RuntimeRunApprovalModeDto::Suggest)
            .with_linked_read_roots(vec![linked_root.clone()])
            .expect("linked roots");
        let request = AutonomousToolRequest::Command(AutonomousCommandRequest {
            argv: vec![
                "ls".into(),
                "-la".into(),
                linked_root.to_string_lossy().into_owned(),
            ],
            cwd: None,
            timeout_ms: Some(1_000),
        });

        let decision = runtime
            .evaluate_safety_policy(
                AUTONOMOUS_TOOL_COMMAND_PROBE,
                &json!({}),
                &request,
                false,
                "input",
            )
            .expect("decision");

        assert_eq!(decision.action, AutonomousSafetyPolicyAction::Allow);
        assert_eq!(decision.code, "policy_allowed_repo_scoped_command");
    }

    #[test]
    fn command_probe_executes_without_operator_review_in_suggest_mode_for_linked_context_path() {
        let tempdir = tempdir().expect("tempdir");
        let repo_root = tempdir.path().join("repo");
        let linked_root = tempdir.path().join("linked-project");
        fs::create_dir_all(&repo_root).expect("repo root");
        fs::create_dir_all(&linked_root).expect("linked root");
        fs::write(linked_root.join("README.md"), "linked project").expect("linked file");
        let runtime = test_runtime(&repo_root, RuntimeRunApprovalModeDto::Suggest)
            .with_linked_read_roots(vec![linked_root.clone()])
            .expect("linked roots");

        let result = runtime
            .command_with_approval_for_tool(
                AUTONOMOUS_TOOL_COMMAND_PROBE,
                AutonomousCommandRequest {
                    argv: vec!["ls".into(), linked_root.to_string_lossy().into_owned()],
                    cwd: None,
                    timeout_ms: Some(1_000),
                },
                false,
            )
            .expect("command_probe should run without operator approval");

        let AutonomousToolOutput::Command(output) = result.output else {
            panic!("expected command output");
        };
        assert!(output.spawned);
        assert_eq!(output.exit_code, Some(0));
        assert_eq!(
            output.policy.outcome,
            AutonomousCommandPolicyOutcome::Allowed
        );
        assert_eq!(output.policy.code, "policy_allowed_repo_scoped_command");
    }

    #[test]
    fn command_probe_and_verify_scope_mismatches_are_model_fixable() {
        let tempdir = tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path(), RuntimeRunApprovalModeDto::Suggest);
        let scaffold_request = AutonomousCommandRequest {
            argv: vec![
                "npm".into(),
                "create".into(),
                "vite@latest".into(),
                ".".into(),
                "--".into(),
                "--template".into(),
                "react-ts".into(),
                "--yes".into(),
            ],
            cwd: Some(".".into()),
            timeout_ms: Some(60_000),
        };

        let probe_error = runtime
            .command_with_approval_for_tool(
                AUTONOMOUS_TOOL_COMMAND_PROBE,
                scaffold_request.clone(),
                false,
            )
            .expect_err("scaffold command must not become a command_probe approval");
        assert_eq!(
            probe_error.code,
            "autonomous_tool_command_probe_scope_invalid"
        );
        assert_eq!(probe_error.class, CommandErrorClass::UserFixable);
        assert!(probe_error.message.contains("Use `command_run`"));

        let verify_error = runtime
            .command_with_approval_for_tool(AUTONOMOUS_TOOL_COMMAND_VERIFY, scaffold_request, false)
            .expect_err("scaffold command must not become a command_verify approval");
        assert_eq!(
            verify_error.code,
            "autonomous_tool_command_verify_scope_invalid"
        );
        assert_eq!(verify_error.class, CommandErrorClass::UserFixable);
        assert!(verify_error.message.contains("Use `command_run`"));
    }

    #[test]
    fn full_access_allows_scaffold_command_probe_without_review() {
        let tempdir = tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path(), RuntimeRunApprovalModeDto::Yolo);
        let request = AutonomousToolRequest::Command(AutonomousCommandRequest {
            argv: vec![
                "npm".into(),
                "create".into(),
                "vite@latest".into(),
                ".".into(),
                "--".into(),
                "--template".into(),
                "react-ts".into(),
                "--yes".into(),
            ],
            cwd: Some(".".into()),
            timeout_ms: Some(60_000),
        });

        let decision = runtime
            .evaluate_safety_policy(
                AUTONOMOUS_TOOL_COMMAND_PROBE,
                &json!({"argv": ["npm", "create", "vite@latest", ".", "--", "--template", "react-ts", "--yes"], "cwd": "."}),
                &request,
                false,
                "input-hash",
            )
            .expect("full-access scaffold probe policy");

        assert_eq!(decision.action, AutonomousSafetyPolicyAction::Allow);
        assert_eq!(decision.code, "policy_allowed_full_access_command");
        assert!(decision
            .explanation
            .contains("without command-classifier restrictions"));
    }

    #[test]
    fn command_probe_allows_common_version_discovery_without_review() {
        let tempdir = tempdir().expect("tempdir");
        let runtime = test_runtime(tempdir.path(), RuntimeRunApprovalModeDto::Suggest);

        for argv in [
            ["node", "--version"],
            ["node", "-v"],
            ["npm", "--version"],
            ["npm", "-v"],
            ["pnpm", "--version"],
            ["yarn", "--version"],
            ["bun", "--version"],
            ["python3", "-V"],
            ["cargo", "--version"],
            ["rustc", "--version"],
        ] {
            let request = AutonomousToolRequest::Command(AutonomousCommandRequest {
                argv: argv.into_iter().map(str::to_owned).collect(),
                cwd: Some(".".into()),
                timeout_ms: Some(5_000),
            });

            let decision = runtime
                .evaluate_safety_policy(
                    AUTONOMOUS_TOOL_COMMAND_PROBE,
                    &json!({"argv": argv, "cwd": "."}),
                    &request,
                    false,
                    "input-hash",
                )
                .expect("version probe policy");

            assert_eq!(
                decision.action,
                AutonomousSafetyPolicyAction::Allow,
                "{argv:?} should be allowed as command_probe discovery"
            );
            assert_eq!(decision.code, "policy_allowed_repo_scoped_command");
        }
    }

    #[test]
    fn safety_policy_denies_command_probe_outside_linked_context_path() {
        let tempdir = tempdir().expect("tempdir");
        let repo_root = tempdir.path().join("repo");
        let linked_root = tempdir.path().join("linked-project");
        let outside_root = tempdir.path().join("outside-project");
        fs::create_dir_all(&repo_root).expect("repo root");
        fs::create_dir_all(&linked_root).expect("linked root");
        fs::create_dir_all(&outside_root).expect("outside root");
        let runtime = test_runtime(&repo_root, RuntimeRunApprovalModeDto::Yolo)
            .with_linked_read_roots(vec![linked_root])
            .expect("linked roots");
        let request = AutonomousToolRequest::Command(AutonomousCommandRequest {
            argv: vec![
                "ls".into(),
                "-la".into(),
                outside_root.to_string_lossy().into_owned(),
            ],
            cwd: None,
            timeout_ms: Some(1_000),
        });

        let error = runtime
            .evaluate_safety_policy(
                AUTONOMOUS_TOOL_COMMAND_PROBE,
                &json!({}),
                &request,
                false,
                "input",
            )
            .expect_err("outside path should fail policy validation");

        assert_eq!(error.code, "policy_denied_argument_outside_repo");
        assert!(error.message.contains("linked context paths"));
    }

    #[test]
    fn safety_policy_keeps_command_probe_readonly_and_verify_scoped() {
        let tempdir = tempdir().expect("tempdir");
        fs::write(
            tempdir.path().join("package.json"),
            r#"{"scripts":{"type-check":"tsc --noEmit","type-check:ci":"tsc --noEmit"}}"#,
        )
        .expect("package");
        let scoped_runtime = test_runtime(tempdir.path(), RuntimeRunApprovalModeDto::Suggest);
        let full_access_runtime = test_runtime(tempdir.path(), RuntimeRunApprovalModeDto::Yolo);
        let probe_request = AutonomousToolRequest::Command(AutonomousCommandRequest {
            argv: vec!["cargo".into(), "test".into()],
            cwd: None,
            timeout_ms: None,
        });

        let probe_decision = scoped_runtime
            .evaluate_safety_policy(
                AUTONOMOUS_TOOL_COMMAND_PROBE,
                &json!({"argv": ["cargo", "test"]}),
                &probe_request,
                false,
                "input-hash",
            )
            .expect("probe policy");

        assert_eq!(
            probe_decision.action,
            AutonomousSafetyPolicyAction::RequireApproval
        );
        assert_eq!(probe_decision.code, "policy_escalated_command_probe_scope");

        let verify_decision = full_access_runtime
            .evaluate_safety_policy(
                AUTONOMOUS_TOOL_COMMAND_VERIFY,
                &json!({"argv": ["cargo", "test"]}),
                &probe_request,
                false,
                "input-hash",
            )
            .expect("verify policy");

        assert_eq!(verify_decision.action, AutonomousSafetyPolicyAction::Allow);

        let root_cwd_verify_request = AutonomousToolRequest::Command(AutonomousCommandRequest {
            argv: vec!["cargo".into(), "test".into()],
            cwd: Some(".".into()),
            timeout_ms: None,
        });
        let root_cwd_verify_decision = full_access_runtime
            .evaluate_safety_policy(
                AUTONOMOUS_TOOL_COMMAND_VERIFY,
                &json!({"argv": ["cargo", "test"], "cwd": "."}),
                &root_cwd_verify_request,
                false,
                "input-hash",
            )
            .expect("verify policy with root cwd shorthand");

        assert_eq!(
            root_cwd_verify_decision.action,
            AutonomousSafetyPolicyAction::Allow
        );

        let type_check_request = AutonomousToolRequest::Command(AutonomousCommandRequest {
            argv: vec![
                "pnpm".into(),
                "--filter".into(),
                "client".into(),
                "type-check".into(),
            ],
            cwd: None,
            timeout_ms: None,
        });
        let type_check_decision = full_access_runtime
            .evaluate_safety_policy(
                AUTONOMOUS_TOOL_COMMAND_VERIFY,
                &json!({"argv": ["pnpm", "--filter", "client", "type-check"]}),
                &type_check_request,
                false,
                "input-hash",
            )
            .expect("scoped type-check verify policy");

        assert_eq!(
            type_check_decision.action,
            AutonomousSafetyPolicyAction::Allow
        );

        let type_check_variant_request = AutonomousToolRequest::Command(AutonomousCommandRequest {
            argv: vec!["pnpm".into(), "run".into(), "type-check:ci".into()],
            cwd: None,
            timeout_ms: None,
        });
        let scoped_type_check_variant_decision = scoped_runtime
            .evaluate_safety_policy(
                AUTONOMOUS_TOOL_COMMAND_VERIFY,
                &json!({"argv": ["pnpm", "run", "type-check:ci"]}),
                &type_check_variant_request,
                false,
                "input-hash",
            )
            .expect("type-check variant verify policy");

        assert_eq!(
            scoped_type_check_variant_decision.action,
            AutonomousSafetyPolicyAction::RequireApproval
        );

        let full_access_type_check_variant_decision = full_access_runtime
            .evaluate_safety_policy(
                AUTONOMOUS_TOOL_COMMAND_VERIFY,
                &json!({"argv": ["pnpm", "run", "type-check:ci"]}),
                &type_check_variant_request,
                false,
                "input-hash",
            )
            .expect("full-access type-check variant verify policy");

        assert_eq!(
            full_access_type_check_variant_decision.action,
            AutonomousSafetyPolicyAction::Allow
        );
        assert_eq!(
            full_access_type_check_variant_decision.code,
            "policy_allowed_full_access_command"
        );
    }

    #[test]
    fn host_command_denied_until_owner_admin_mode_is_active() {
        let tempdir = tempdir().expect("tempdir");
        let app_data = tempdir.path();
        let repo_root = app_data.join("computer-use");
        fs::create_dir_all(&repo_root).expect("repo");
        fs::create_dir_all(app_data.join("desktop-control")).expect("settings dir");
        fs::write(
            app_data.join("desktop-control").join("settings.json"),
            r#"{"cloudStreamingEnabled":false,"manualCloudControlEnabled":false,"policyProfile":"default_safe","ownerAdminExpiresAt":null,"updatedAt":null}"#,
        )
        .expect("settings");
        let runtime = test_runtime_for_agent(
            &repo_root,
            RuntimeRunApprovalModeDto::Yolo,
            RuntimeAgentIdDto::ComputerUse,
        );
        let request =
            AutonomousToolRequest::HostCommand(super::super::AutonomousHostCommandRequest {
                argv: vec!["echo".into(), "hello".into()],
                cwd: Some(app_data.to_string_lossy().into_owned()),
                timeout_ms: Some(1_000),
                preview: true,
                preview_token: None,
                reason: Some("check owner admin gate".into()),
                rollback_hints: Vec::new(),
            });

        let decision = runtime
            .evaluate_safety_policy(
                AUTONOMOUS_TOOL_HOST_COMMAND,
                &json!({}),
                &request,
                false,
                "input",
            )
            .expect("decision");

        assert_eq!(decision.action, AutonomousSafetyPolicyAction::Deny);
        assert_eq!(decision.code, "policy_denied_owner_admin_mode_inactive");
    }

    #[test]
    fn host_command_preview_allowed_when_owner_admin_mode_is_active() {
        let tempdir = tempdir().expect("tempdir");
        let app_data = tempdir.path();
        let repo_root = app_data.join("computer-use");
        fs::create_dir_all(&repo_root).expect("repo");
        fs::create_dir_all(app_data.join("desktop-control")).expect("settings dir");
        fs::write(
            app_data.join("desktop-control").join("settings.json"),
            r#"{"cloudStreamingEnabled":false,"manualCloudControlEnabled":false,"policyProfile":"owner_admin","ownerAdminExpiresAt":"2999-01-01T00:00:00Z","updatedAt":null}"#,
        )
        .expect("settings");
        let runtime = test_runtime_for_agent(
            &repo_root,
            RuntimeRunApprovalModeDto::Yolo,
            RuntimeAgentIdDto::ComputerUse,
        );
        let request =
            AutonomousToolRequest::HostCommand(super::super::AutonomousHostCommandRequest {
                argv: vec!["echo".into(), "hello".into()],
                cwd: Some(app_data.to_string_lossy().into_owned()),
                timeout_ms: Some(1_000),
                preview: true,
                preview_token: None,
                reason: Some("preview host command".into()),
                rollback_hints: Vec::new(),
            });

        let decision = runtime
            .evaluate_safety_policy(
                AUTONOMOUS_TOOL_HOST_COMMAND,
                &json!({}),
                &request,
                false,
                "input",
            )
            .expect("decision");

        assert_eq!(decision.action, AutonomousSafetyPolicyAction::Allow);
        assert_eq!(decision.code, "policy_allowed_host_command_preview");
    }

    #[test]
    fn high_impact_host_command_requires_preview_token_before_approval() {
        let tempdir = tempdir().expect("tempdir");
        let app_data = tempdir.path();
        let repo_root = app_data.join("computer-use");
        fs::create_dir_all(&repo_root).expect("repo");
        fs::create_dir_all(app_data.join("desktop-control")).expect("settings dir");
        fs::write(
            app_data.join("desktop-control").join("settings.json"),
            r#"{"cloudStreamingEnabled":false,"manualCloudControlEnabled":false,"policyProfile":"owner_admin","ownerAdminExpiresAt":"2999-01-01T00:00:00Z","updatedAt":null}"#,
        )
        .expect("settings");
        let runtime = test_runtime_for_agent(
            &repo_root,
            RuntimeRunApprovalModeDto::Yolo,
            RuntimeAgentIdDto::ComputerUse,
        );
        let request =
            AutonomousToolRequest::HostCommand(super::super::AutonomousHostCommandRequest {
                argv: vec!["echo".into(), "install".into()],
                cwd: Some(app_data.to_string_lossy().into_owned()),
                timeout_ms: Some(1_000),
                preview: false,
                preview_token: None,
                reason: Some("simulate high-impact package operation".into()),
                rollback_hints: vec!["package operation".into()],
            });

        let decision = runtime
            .evaluate_safety_policy(
                AUTONOMOUS_TOOL_HOST_COMMAND,
                &json!({}),
                &request,
                true,
                "input",
            )
            .expect("decision");

        assert_eq!(
            decision.action,
            AutonomousSafetyPolicyAction::RequireApproval
        );
        assert_eq!(decision.code, "policy_requires_host_command_preview");

        let AutonomousToolRequest::HostCommand(host_request) = request else {
            panic!("host command request");
        };
        let result = runtime
            .host_command_with_operator_approval(host_request)
            .expect("unspawned host command result");
        let AutonomousToolOutput::Command(output) = result.output else {
            panic!("command output");
        };
        assert!(!output.spawned);
        assert_eq!(output.policy.code, "policy_requires_host_command_preview");
        assert!(output.preview_token.is_none());
        let impact = output
            .host_command_impact
            .expect("host command impact metadata");
        assert!(impact.requires_preview);
        assert_eq!(impact.preview_token_validated, None);
        assert!(impact
            .detected_surfaces
            .iter()
            .any(|surface| surface.category == "package_manager"));
        assert!(impact.elevation.uses_os_native_prompt);
        assert!(!impact.elevation.bypasses_os_protection);
    }

    #[test]
    fn high_impact_host_command_preview_token_unlocks_approval_request() {
        let tempdir = tempdir().expect("tempdir");
        let app_data = tempdir.path();
        let repo_root = app_data.join("computer-use");
        fs::create_dir_all(&repo_root).expect("repo");
        fs::create_dir_all(app_data.join("desktop-control")).expect("settings dir");
        fs::write(
            app_data.join("desktop-control").join("settings.json"),
            r#"{"cloudStreamingEnabled":false,"manualCloudControlEnabled":false,"policyProfile":"owner_admin","ownerAdminExpiresAt":"2999-01-01T00:00:00Z","updatedAt":null}"#,
        )
        .expect("settings");
        let runtime = test_runtime_for_agent(
            &repo_root,
            RuntimeRunApprovalModeDto::Yolo,
            RuntimeAgentIdDto::ComputerUse,
        );
        let preview = runtime
            .host_command(super::super::AutonomousHostCommandRequest {
                argv: vec!["echo".into(), "install".into()],
                cwd: Some(app_data.to_string_lossy().into_owned()),
                timeout_ms: Some(1_000),
                preview: true,
                preview_token: None,
                reason: Some("simulate high-impact package operation".into()),
                rollback_hints: vec!["package operation".into()],
            })
            .expect("preview result");
        let AutonomousToolOutput::Command(preview_output) = preview.output else {
            panic!("command output");
        };
        assert!(!preview_output.spawned);
        let preview_token = preview_output
            .preview_token
            .expect("preview token for high-impact command");

        let host_request = super::super::AutonomousHostCommandRequest {
            argv: vec!["echo".into(), "install".into()],
            cwd: Some(app_data.to_string_lossy().into_owned()),
            timeout_ms: Some(1_000),
            preview: false,
            preview_token: Some(preview_token),
            reason: Some("simulate high-impact package operation".into()),
            rollback_hints: vec!["package operation".into()],
        };
        let review_result = runtime
            .host_command(host_request.clone())
            .expect("review result after valid preview token");
        let AutonomousToolOutput::Command(review_output) = review_result.output else {
            panic!("command output");
        };
        assert!(!review_output.spawned);
        let impact = review_output
            .host_command_impact
            .expect("host command impact metadata");
        assert_eq!(impact.preview_token_validated, Some(true));
        assert!(impact
            .elevation
            .protected_boundaries
            .iter()
            .any(|boundary| boundary == "windows_uac"));
        assert!(review_output
            .suggested_next_actions
            .iter()
            .any(|action| action.contains("will not automate or bypass")));

        let request = AutonomousToolRequest::HostCommand(host_request);
        let decision = runtime
            .evaluate_safety_policy(
                AUTONOMOUS_TOOL_HOST_COMMAND,
                &json!({}),
                &request,
                false,
                "input",
            )
            .expect("decision");

        assert_eq!(
            decision.action,
            AutonomousSafetyPolicyAction::RequireApproval
        );
        assert_eq!(decision.code, "policy_escalated_owner_admin_host_command");
    }

    #[test]
    fn host_command_preview_reports_structured_impact_and_elevation_boundaries() {
        let tempdir = tempdir().expect("tempdir");
        let app_data = tempdir.path();
        let repo_root = app_data.join("computer-use");
        fs::create_dir_all(&repo_root).expect("repo");
        fs::create_dir_all(app_data.join("desktop-control")).expect("settings dir");
        fs::write(
            app_data.join("desktop-control").join("settings.json"),
            r#"{"cloudStreamingEnabled":false,"manualCloudControlEnabled":false,"policyProfile":"owner_admin","ownerAdminExpiresAt":"2999-01-01T00:00:00Z","updatedAt":null}"#,
        )
        .expect("settings");
        let runtime = test_runtime_for_agent(
            &repo_root,
            RuntimeRunApprovalModeDto::Yolo,
            RuntimeAgentIdDto::ComputerUse,
        );

        let preview = runtime
            .host_command(super::super::AutonomousHostCommandRequest {
                argv: vec!["winget".into(), "install".into(), "Git.Git".into()],
                cwd: Some(app_data.to_string_lossy().into_owned()),
                timeout_ms: Some(1_000),
                preview: true,
                preview_token: None,
                reason: Some("preview workstation package installation".into()),
                rollback_hints: vec!["package: Git.Git".into()],
            })
            .expect("preview result");
        let AutonomousToolOutput::Command(output) = preview.output else {
            panic!("command output");
        };
        let impact = output
            .host_command_impact
            .expect("host command impact metadata");

        assert_eq!(impact.schema, "xero.host_command_impact.v1");
        assert_eq!(
            impact.policy_profile,
            AutonomousCommandPolicyProfile::DependencyInstallation
        );
        assert!(impact.requires_preview);
        assert!(impact.requires_owner_approval);
        assert_eq!(impact.preview_token_validated, None);
        assert_eq!(impact.rollback_hints, vec!["package: Git.Git"]);
        assert!(impact.detected_surfaces.iter().any(|surface| {
            surface.category == "package_manager" && surface.evidence == "winget"
        }));
        assert!(impact.elevation.uses_os_native_prompt);
        assert!(!impact.elevation.bypasses_os_protection);
        assert!(impact
            .elevation
            .protected_boundaries
            .iter()
            .any(|boundary| boundary == "macos_tcc"));
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
        test_runtime_for_agent(repo_root, approval_mode, RuntimeAgentIdDto::Engineer)
    }

    fn test_runtime_for_agent(
        repo_root: &Path,
        approval_mode: RuntimeRunApprovalModeDto,
        runtime_agent_id: RuntimeAgentIdDto,
    ) -> AutonomousToolRuntime {
        AutonomousToolRuntime::new(repo_root)
            .expect("runtime")
            .with_runtime_run_controls(RuntimeRunControlStateDto {
                active: RuntimeRunActiveControlSnapshotDto {
                    runtime_agent_id,
                    agent_definition_id: None,
                    agent_definition_version: None,
                    provider_profile_id: None,
                    model_id: "test-model".into(),
                    thinking_effort: None,
                    approval_mode,
                    plan_mode_required: false,
                    auto_compact_enabled: true,
                    revision: 1,
                    applied_at: "2026-04-30T00:00:00Z".into(),
                },
                pending: None,
            })
    }
}
