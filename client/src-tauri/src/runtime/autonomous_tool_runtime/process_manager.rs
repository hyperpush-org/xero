use super::{
    policy::process_manager_policy_trace, process::apply_sanitized_command_environment,
    repo_scope::normalize_relative_path, AutonomousProcessActionRiskLevel,
    AutonomousProcessLifecycleContract, AutonomousProcessManagerAction,
    AutonomousProcessManagerContract, AutonomousProcessManagerOutput,
    AutonomousProcessManagerRequest, AutonomousProcessMetadata, AutonomousProcessOutputChunk,
    AutonomousProcessOutputLimits, AutonomousProcessPersistenceContract, AutonomousToolOutput,
    AutonomousToolResult, AutonomousToolRuntime, AUTONOMOUS_TOOL_PROCESS_MANAGER,
};
use crate::{
    commands::{validate_non_empty, CommandError, CommandResult},
    runtime::redaction::redact_command_argv_for_persistence,
};

const PROCESS_MANAGER_PHASE: &str = "phase_0_contract";
const RECENT_OUTPUT_RING_BYTES: usize = 1024 * 1024;
const RECENT_OUTPUT_RING_CHUNKS: usize = 512;
const FULL_OUTPUT_ARTIFACT_THRESHOLD_BYTES: usize = 1024 * 1024;
const PROCESS_OUTPUT_EXCERPT_BYTES: usize = 16 * 1024;

impl AutonomousToolRuntime {
    pub fn process_manager(
        &self,
        request: AutonomousProcessManagerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        validate_process_manager_request(&request)?;

        let action = request.action;
        let process_id = request
            .process_id
            .as_deref()
            .map(str::trim)
            .and_then(|value| {
                if value.is_empty() {
                    None
                } else {
                    Some(value.to_owned())
                }
            });
        let policy =
            process_manager_policy_trace(action, request.target_ownership, request.persistent);
        let message = format!(
            "Process manager action `{}` matched the phase-0 contract; no process control was executed.",
            process_manager_action_label(action)
        );

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_PROCESS_MANAGER.into(),
            summary: message.clone(),
            command_result: None,
            output: AutonomousToolOutput::ProcessManager(AutonomousProcessManagerOutput {
                action,
                phase: PROCESS_MANAGER_PHASE.into(),
                spawned: false,
                process_id,
                processes: Vec::<AutonomousProcessMetadata>::new(),
                chunks: Vec::<AutonomousProcessOutputChunk>::new(),
                next_cursor: request.after_cursor,
                policy,
                contract: process_manager_contract(),
                message,
            }),
        })
    }
}

fn validate_process_manager_request(
    request: &AutonomousProcessManagerRequest,
) -> CommandResult<()> {
    match request.action {
        AutonomousProcessManagerAction::Start => {
            if request.argv.is_empty() || request.argv[0].trim().is_empty() {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_process_manager_start_invalid",
                    "Cadence requires process_manager start requests to include a non-empty argv[0].",
                ));
            }
            validate_argv_contract(&request.argv)?;
        }
        AutonomousProcessManagerAction::Status
        | AutonomousProcessManagerAction::Output
        | AutonomousProcessManagerAction::Digest
        | AutonomousProcessManagerAction::WaitForReady
        | AutonomousProcessManagerAction::Signal
        | AutonomousProcessManagerAction::Kill
        | AutonomousProcessManagerAction::Restart => {
            validate_non_empty(
                request.process_id.as_deref().unwrap_or_default(),
                "processId",
            )?;
        }
        AutonomousProcessManagerAction::Send | AutonomousProcessManagerAction::SendAndWait => {
            validate_non_empty(
                request.process_id.as_deref().unwrap_or_default(),
                "processId",
            )?;
            validate_non_empty(request.input.as_deref().unwrap_or_default(), "input")?;
        }
        AutonomousProcessManagerAction::GroupStatus => {
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
    if let Some(wait_url) = request.wait_url.as_deref() {
        validate_non_empty(wait_url, "waitUrl")?;
    }

    Ok(())
}

fn validate_argv_contract(argv: &[String]) -> CommandResult<()> {
    if argv.iter().any(|argument| argument.contains('\0')) {
        return Err(CommandError::user_fixable(
            "autonomous_tool_process_manager_start_invalid",
            "Cadence refused a process_manager command that contained a NUL byte.",
        ));
    }

    let _redacted = redact_command_argv_for_persistence(argv);
    let mut probe = std::process::Command::new(&argv[0]);
    apply_sanitized_command_environment(&mut probe);
    Ok(())
}

pub(super) fn process_manager_contract() -> AutonomousProcessManagerContract {
    AutonomousProcessManagerContract {
        phase: PROCESS_MANAGER_PHASE.into(),
        supported_actions: vec![
            AutonomousProcessManagerAction::Start,
            AutonomousProcessManagerAction::List,
            AutonomousProcessManagerAction::Status,
            AutonomousProcessManagerAction::Output,
            AutonomousProcessManagerAction::Digest,
            AutonomousProcessManagerAction::WaitForReady,
            AutonomousProcessManagerAction::Send,
            AutonomousProcessManagerAction::SendAndWait,
            AutonomousProcessManagerAction::Signal,
            AutonomousProcessManagerAction::Kill,
            AutonomousProcessManagerAction::Restart,
            AutonomousProcessManagerAction::GroupStatus,
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
            app_shutdown: "terminate_non_persistent_cadence_owned_process_trees".into(),
            thread_switch: "reinject_owned_process_digest_without_granting_new_control".into(),
            session_compaction: "persist_metadata_and_reinject_digest_with_output_cursors".into(),
            crash_recovery: "restore_metadata_then_mark_unverified_until_reobserved".into(),
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
        AutonomousProcessManagerAction::Send => "send",
        AutonomousProcessManagerAction::SendAndWait => "send_and_wait",
        AutonomousProcessManagerAction::Signal => "signal",
        AutonomousProcessManagerAction::Kill => "kill",
        AutonomousProcessManagerAction::Restart => "restart",
        AutonomousProcessManagerAction::GroupStatus => "group_status",
    }
}
