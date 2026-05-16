use std::{
    path::{Path, PathBuf},
    thread,
};

use serde_json::Value as JsonValue;
use xero_agent_core::{
    runtime_trace_id_for_run, AgentRuntimeFacade, ApprovalDecisionRequest, CompactSessionRequest,
    ContextManifest as CoreContextManifest, ExportTraceRequest, ForkSessionRequest,
    MessageRole as CoreMessageRole, RunSnapshot as CoreRunSnapshot, RunStatus as CoreRunStatus,
    RuntimeEvent as CoreRuntimeEvent, RuntimeEventKind as CoreRuntimeEventKind,
    RuntimeMessage as CoreRuntimeMessage, RuntimeTrace, RuntimeTraceContext,
};

use super::*;

#[derive(Debug, Clone)]
pub struct DesktopAgentCoreRuntime {
    supervisor: AgentRunSupervisor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopRunDriveMode {
    CreateOnly,
    Background,
}

#[derive(Debug, Clone)]
pub struct DesktopStartRunRequest {
    pub request: OwnedAgentRunRequest,
    pub drive_mode: DesktopRunDriveMode,
}

#[derive(Debug, Clone)]
pub struct DesktopContinueRunRequest {
    pub request: ContinueOwnedAgentRunRequest,
    pub drive_mode: DesktopRunDriveMode,
}

#[derive(Debug, Clone)]
pub struct DesktopCancelRunRequest {
    pub repo_root: PathBuf,
    pub project_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone)]
pub struct DesktopRejectActionRequest {
    pub repo_root: PathBuf,
    pub request: ApprovalDecisionRequest,
}

#[derive(Debug, Clone)]
pub struct DesktopForkSessionRequest {
    pub repo_root: PathBuf,
    pub request: ForkSessionRequest,
    pub source_run_id: Option<String>,
    pub title: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Clone)]
pub struct DesktopCompactSessionRequest {
    pub repo_root: PathBuf,
    pub request: CompactSessionRequest,
    pub run_id: Option<String>,
    pub raw_tail_message_count: Option<u32>,
    pub trigger: project_store::AgentCompactionTrigger,
    pub provider_config: AgentProviderConfig,
}

#[derive(Debug, Clone)]
pub struct DesktopExportTraceRequest {
    pub repo_root: PathBuf,
    pub project_id: String,
    pub run_id: String,
}

impl DesktopAgentCoreRuntime {
    pub fn new(supervisor: AgentRunSupervisor) -> Self {
        Self { supervisor }
    }

    pub fn start_run(
        &self,
        request: OwnedAgentRunRequest,
        drive_mode: DesktopRunDriveMode,
    ) -> CommandResult<AgentRunSnapshotRecord> {
        let snapshot = create_owned_agent_run(&request)?;
        if drive_mode == DesktopRunDriveMode::Background {
            self.spawn_owned_agent_run(request)?;
        }
        Ok(snapshot)
    }

    pub fn continue_run(
        &self,
        request: ContinueOwnedAgentRunRequest,
        drive_mode: DesktopRunDriveMode,
    ) -> CommandResult<PreparedOwnedAgentContinuation> {
        let prepared = prepare_owned_agent_continuation_for_drive(&request)?;
        if drive_mode == DesktopRunDriveMode::Background && prepared.drive_required {
            self.spawn_owned_agent_continuation(
                prepared.snapshot.run.agent_session_id.clone(),
                prepared.drive_request.clone(),
            )?;
        }
        Ok(prepared)
    }

    pub fn cancel_run(
        &self,
        repo_root: PathBuf,
        project_id: String,
        run_id: String,
    ) -> CommandResult<AgentRunSnapshotRecord> {
        let _ = self.supervisor.cancel(&run_id)?;
        cancel_owned_agent_run(&repo_root, &project_id, &run_id)
    }

    pub fn is_active(&self, run_id: &str) -> CommandResult<bool> {
        self.supervisor.is_active(run_id)
    }

    pub fn reject_action(
        &self,
        repo_root: PathBuf,
        request: ApprovalDecisionRequest,
    ) -> CommandResult<AgentRunSnapshotRecord> {
        if self.supervisor.is_active(&request.run_id)? {
            return Err(CommandError::user_fixable(
                "agent_run_already_active",
                format!(
                    "Xero is still driving owned-agent run `{}`. Wait for it to pause before rejecting action `{}`.",
                    request.run_id, request.action_id
                ),
            ));
        }

        let before =
            project_store::load_agent_run(&repo_root, &request.project_id, &request.run_id)?;
        if matches!(
            before.run.status,
            AgentRunStatus::Cancelled
                | AgentRunStatus::HandedOff
                | AgentRunStatus::Completed
                | AgentRunStatus::Failed
        ) {
            return Err(CommandError::user_fixable(
                "agent_run_terminal",
                format!(
                    "Xero cannot reject action `{}` because owned-agent run `{}` is already {:?}.",
                    request.action_id, request.run_id, before.run.status
                ),
            ));
        }

        let rejected = project_store::reject_pending_agent_action_request(
            &repo_root,
            &request.project_id,
            &request.run_id,
            &request.action_id,
            request.response.as_deref(),
        )?;
        append_event(
            &repo_root,
            &request.project_id,
            &request.run_id,
            AgentRunEventKind::PolicyDecision,
            json!({
                "kind": "approval_decision",
                "actionId": rejected.action_id,
                "actionType": rejected.action_type,
                "decision": "rejected",
                "response": rejected.response,
                "status": rejected.status,
            }),
        )?;
        record_state_transition(
            &repo_root,
            &request.project_id,
            &request.run_id,
            AgentStateTransition {
                from: Some(AgentRunState::ApprovalWait),
                to: AgentRunState::Blocked,
                reason: "Operator rejected a pending owned-agent action.",
                stop_reason: Some(AgentRunStopReason::Blocked),
                extra: Some(json!({
                    "actionId": request.action_id,
                    "decision": "rejected",
                })),
            },
        )?;
        append_event(
            &repo_root,
            &request.project_id,
            &request.run_id,
            AgentRunEventKind::RunFailed,
            json!({
                "code": "agent_action_rejected",
                "message": format!("Operator rejected action `{}`.", request.action_id),
                "retryable": false,
                "state": AgentRunState::Blocked.as_str(),
                "stopReason": AgentRunStopReason::Blocked.as_str(),
            }),
        )?;
        project_store::update_agent_run_status(
            &repo_root,
            &request.project_id,
            &request.run_id,
            AgentRunStatus::Failed,
            Some(project_store::AgentRunDiagnosticRecord {
                code: "agent_action_rejected".into(),
                message: format!("Operator rejected action `{}`.", request.action_id),
            }),
            &now_timestamp(),
        )
    }

    pub fn fork_session(
        &self,
        repo_root: PathBuf,
        request: ForkSessionRequest,
        source_run_id: Option<String>,
        title: Option<String>,
        selected: bool,
    ) -> CommandResult<AgentRunSnapshotRecord> {
        let source_run_id = source_run_id_for_session_fork(
            &repo_root,
            &request.project_id,
            &request.source_agent_session_id,
            source_run_id,
        )?;
        let branch = project_store::create_agent_session_branch(
            &repo_root,
            &project_store::AgentSessionBranchCreateRecord {
                project_id: request.project_id.clone(),
                source_agent_session_id: request.source_agent_session_id.clone(),
                source_run_id: source_run_id.clone(),
                target_agent_session_id: Some(request.target_agent_session_id.clone()),
                title,
                selected,
                boundary: project_store::AgentSessionBranchBoundary::Run,
            },
        )?;
        let replay_project_id = branch.replay_run.run.project_id.clone();
        let replay_run_id = branch.replay_run.run.run_id.clone();
        let replay_trace_id = branch.replay_run.run.trace_id.clone();
        let source_trace_id = branch.replay_run.run.parent_trace_id.clone();
        let lineage_id = branch.lineage.lineage_id.clone();
        append_event(
            &repo_root,
            &replay_project_id,
            &replay_run_id,
            AgentRunEventKind::StateTransition,
            json!({
                "kind": "session_forked",
                "sourceAgentSessionId": request.source_agent_session_id,
                "targetAgentSessionId": request.target_agent_session_id,
                "sourceRunId": source_run_id,
                "sourceTraceId": source_trace_id,
                "replayRunId": replay_run_id,
                "replayTraceId": replay_trace_id,
                "lineageId": lineage_id,
            }),
        )?;
        project_store::load_agent_run(&repo_root, &replay_project_id, &replay_run_id)
    }

    pub fn compact_session(
        &self,
        repo_root: PathBuf,
        request: CompactSessionRequest,
        run_id: Option<String>,
        raw_tail_message_count: Option<u32>,
        trigger: project_store::AgentCompactionTrigger,
        provider_config: AgentProviderConfig,
    ) -> CommandResult<AgentRunSnapshotRecord> {
        crate::commands::validate_non_empty(&request.reason, "reason")?;
        let run_id = run_id_for_session_operation(
            &repo_root,
            &request.project_id,
            &request.agent_session_id,
            run_id,
        )?;
        if self.supervisor.is_active(&run_id)? {
            return Err(CommandError::user_fixable(
                "agent_run_already_active",
                format!(
                    "Xero is still driving owned-agent run `{run_id}`. Wait for it to pause or finish before compacting the session."
                ),
            ));
        }
        let provider = create_provider_adapter(provider_config)?;
        let compaction = crate::commands::session_history::compact_session_history_with_provider(
            &repo_root,
            &request.project_id,
            &request.agent_session_id,
            Some(&run_id),
            raw_tail_message_count,
            trigger,
            &request.reason,
            provider.as_ref(),
        )?;
        let snapshot = project_store::load_agent_run(&repo_root, &request.project_id, &run_id)?;
        persist_compaction_context_manifest(&repo_root, &snapshot, &compaction)?;
        append_event(
            &repo_root,
            &request.project_id,
            &run_id,
            AgentRunEventKind::PolicyDecision,
            json!({
                "kind": "session_compaction",
                "action": "compacted",
                "compactionId": compaction.compaction_id,
                "reason": request.reason,
                "rawTailMessageCount": compaction.raw_tail_message_count,
                "coveredRunIds": compaction.covered_run_ids,
                "coveredMessageStartId": compaction.covered_message_start_id,
                "coveredMessageEndId": compaction.covered_message_end_id,
                "coveredEventStartId": compaction.covered_event_start_id,
                "coveredEventEndId": compaction.covered_event_end_id,
            }),
        )?;
        project_store::load_agent_run(&repo_root, &request.project_id, &run_id)
    }

    pub fn spawn_owned_agent_run(&self, request: OwnedAgentRunRequest) -> CommandResult<()> {
        let lease = self.supervisor.begin(
            &request.project_id,
            &request.agent_session_id,
            &request.run_id,
        )?;
        thread::spawn(move || {
            let token = lease.token();
            let _ = drive_owned_agent_run(request, token);
            drop(lease);
        });
        Ok(())
    }

    pub fn spawn_owned_agent_continuation(
        &self,
        agent_session_id: String,
        request: ContinueOwnedAgentRunRequest,
    ) -> CommandResult<()> {
        let lease =
            self.supervisor
                .begin(&request.project_id, &agent_session_id, &request.run_id)?;
        thread::spawn(move || {
            let token = lease.token();
            let _ = drive_owned_agent_continuation(request, token);
            drop(lease);
        });
        Ok(())
    }

    pub fn export_trace(
        &self,
        repo_root: PathBuf,
        project_id: String,
        run_id: String,
    ) -> CommandResult<RuntimeTrace> {
        let snapshot = project_store::load_agent_run(&repo_root, &project_id, &run_id)?;
        let context_manifests =
            project_store::list_agent_context_manifests_for_run(&repo_root, &project_id, &run_id)?;
        RuntimeTrace::from_snapshot(core_snapshot_from_desktop(snapshot, context_manifests))
            .map_err(|error| {
                CommandError::system_fault(
                    error.code,
                    format!(
                        "Xero could not export the runtime protocol trace: {}",
                        error.message
                    ),
                )
            })
    }
}

impl AgentRuntimeFacade for DesktopAgentCoreRuntime {
    type StartRunRequest = DesktopStartRunRequest;
    type ContinueRunRequest = DesktopContinueRunRequest;
    type UserInputRequest = DesktopContinueRunRequest;
    type ApprovalRequest = DesktopContinueRunRequest;
    type RejectRequest = DesktopRejectActionRequest;
    type CancelRunRequest = DesktopCancelRunRequest;
    type ResumeRunRequest = DesktopContinueRunRequest;
    type ForkSessionRequest = DesktopForkSessionRequest;
    type CompactSessionRequest = DesktopCompactSessionRequest;
    type ExportTraceRequest = DesktopExportTraceRequest;
    type Snapshot = AgentRunSnapshotRecord;
    type Trace = RuntimeTrace;
    type Error = CommandError;

    fn start_run(&self, request: DesktopStartRunRequest) -> CommandResult<AgentRunSnapshotRecord> {
        self.start_run(request.request, request.drive_mode)
    }

    fn continue_run(
        &self,
        request: DesktopContinueRunRequest,
    ) -> CommandResult<AgentRunSnapshotRecord> {
        self.continue_run(request.request, request.drive_mode)
            .map(|prepared| prepared.snapshot)
    }

    fn submit_user_input(
        &self,
        request: DesktopContinueRunRequest,
    ) -> CommandResult<AgentRunSnapshotRecord> {
        <Self as AgentRuntimeFacade>::continue_run(self, request)
    }

    fn approve_action(
        &self,
        request: DesktopContinueRunRequest,
    ) -> CommandResult<AgentRunSnapshotRecord> {
        <Self as AgentRuntimeFacade>::continue_run(self, request)
    }

    fn reject_action(
        &self,
        request: DesktopRejectActionRequest,
    ) -> CommandResult<AgentRunSnapshotRecord> {
        self.reject_action(request.repo_root, request.request)
    }

    fn cancel_run(
        &self,
        request: DesktopCancelRunRequest,
    ) -> CommandResult<AgentRunSnapshotRecord> {
        self.cancel_run(request.repo_root, request.project_id, request.run_id)
    }

    fn resume_run(
        &self,
        request: DesktopContinueRunRequest,
    ) -> CommandResult<AgentRunSnapshotRecord> {
        <Self as AgentRuntimeFacade>::continue_run(self, request)
    }

    fn fork_session(
        &self,
        request: DesktopForkSessionRequest,
    ) -> CommandResult<AgentRunSnapshotRecord> {
        self.fork_session(
            request.repo_root,
            request.request,
            request.source_run_id,
            request.title,
            request.selected,
        )
    }

    fn compact_session(
        &self,
        request: DesktopCompactSessionRequest,
    ) -> CommandResult<AgentRunSnapshotRecord> {
        self.compact_session(
            request.repo_root,
            request.request,
            request.run_id,
            request.raw_tail_message_count,
            request.trigger,
            request.provider_config,
        )
    }

    fn export_trace(&self, request: DesktopExportTraceRequest) -> CommandResult<RuntimeTrace> {
        self.export_trace(request.repo_root, request.project_id, request.run_id)
    }
}

impl From<DesktopExportTraceRequest> for ExportTraceRequest {
    fn from(request: DesktopExportTraceRequest) -> Self {
        Self {
            project_id: request.project_id,
            run_id: request.run_id,
        }
    }
}

fn source_run_id_for_session_fork(
    repo_root: &Path,
    project_id: &str,
    source_agent_session_id: &str,
    source_run_id: Option<String>,
) -> CommandResult<String> {
    if let Some(source_run_id) = source_run_id {
        crate::commands::validate_non_empty(&source_run_id, "sourceRunId")?;
        return Ok(source_run_id);
    }
    run_id_for_session_operation(repo_root, project_id, source_agent_session_id, None)
}

fn run_id_for_session_operation(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    run_id: Option<String>,
) -> CommandResult<String> {
    if let Some(run_id) = run_id {
        crate::commands::validate_non_empty(&run_id, "runId")?;
        return Ok(run_id);
    }
    let session = project_store::get_agent_session(repo_root, project_id, agent_session_id)?
        .ok_or_else(|| {
            CommandError::user_fixable(
                "agent_session_not_found",
                format!(
                    "Xero could not find agent session `{agent_session_id}` for project `{project_id}`."
                ),
            )
        })?;
    session.last_run_id.ok_or_else(|| {
        CommandError::user_fixable(
            "agent_session_has_no_runs",
            format!(
                "Xero cannot operate on session `{agent_session_id}` because it has no owned-agent runs yet."
            ),
        )
    })
}

fn persist_compaction_context_manifest(
    repo_root: &Path,
    snapshot: &AgentRunSnapshotRecord,
    compaction: &crate::commands::SessionCompactionRecordDto,
) -> CommandResult<()> {
    let existing_manifests = project_store::list_agent_context_manifests_for_run(
        repo_root,
        &snapshot.run.project_id,
        &snapshot.run.run_id,
    )?;
    let turn_index = existing_manifests.len();
    let manifest_id = format!(
        "context-manifest:{}:compact:{}",
        snapshot.run.run_id, compaction.compaction_id
    );
    let manifest = json!({
        "kind": "session_compaction_artifact",
        "schema": "xero.session_compaction_artifact.v1",
        "schemaVersion": 1,
        "projectId": snapshot.run.project_id,
        "agentSessionId": snapshot.run.agent_session_id,
        "runId": snapshot.run.run_id,
        "providerId": snapshot.run.provider_id,
        "modelId": snapshot.run.model_id,
        "turnIndex": turn_index,
        "compactionId": compaction.compaction_id,
        "policyReason": compaction.policy_reason,
        "trigger": compaction.trigger,
        "sourceHash": compaction.source_hash,
        "sourceRunId": compaction.source_run_id,
        "coveredRunIds": compaction.covered_run_ids,
        "coveredMessageStartId": compaction.covered_message_start_id,
        "coveredMessageEndId": compaction.covered_message_end_id,
        "coveredEventStartId": compaction.covered_event_start_id,
        "coveredEventEndId": compaction.covered_event_end_id,
        "rawTailMessageCount": compaction.raw_tail_message_count,
        "summaryTokens": compaction.summary_tokens,
        "inputTokens": compaction.input_tokens,
        "summary": compaction.summary,
    });
    project_store::insert_agent_context_manifest(
        repo_root,
        &project_store::NewAgentContextManifestRecord {
            manifest_id,
            project_id: snapshot.run.project_id.clone(),
            agent_session_id: snapshot.run.agent_session_id.clone(),
            run_id: Some(snapshot.run.run_id.clone()),
            runtime_agent_id: snapshot.run.runtime_agent_id,
            agent_definition_id: snapshot.run.agent_definition_id.clone(),
            agent_definition_version: snapshot.run.agent_definition_version,
            provider_id: Some(snapshot.run.provider_id.clone()),
            model_id: Some(snapshot.run.model_id.clone()),
            request_kind: project_store::AgentContextManifestRequestKind::Diagnostic,
            policy_action: project_store::AgentContextPolicyAction::CompactNow,
            policy_reason_code: compaction.policy_reason.clone(),
            budget_tokens: None,
            estimated_tokens: compaction.summary_tokens,
            pressure: project_store::AgentContextBudgetPressure::Unknown,
            context_hash: compaction.source_hash.clone(),
            included_contributors: vec![project_store::AgentContextManifestContributorRecord {
                contributor_id: format!("compaction_summary:{}", compaction.compaction_id),
                kind: "compaction_summary".into(),
                source_id: Some(compaction.compaction_id.clone()),
                estimated_tokens: compaction.summary_tokens,
                reason: Some(compaction.policy_reason.clone()),
            }],
            excluded_contributors: Vec::new(),
            retrieval_query_ids: Vec::new(),
            retrieval_result_ids: Vec::new(),
            compaction_id: Some(compaction.compaction_id.clone()),
            handoff_id: None,
            redaction_state: project_store::AgentContextRedactionState::Clean,
            manifest,
            created_at: now_timestamp(),
        },
    )?;
    Ok(())
}

fn core_snapshot_from_desktop(
    snapshot: AgentRunSnapshotRecord,
    context_manifests: Vec<project_store::AgentContextManifestRecord>,
) -> CoreRunSnapshot {
    CoreRunSnapshot {
        trace_id: runtime_trace_id_for_run(&snapshot.run.project_id, &snapshot.run.run_id),
        runtime_agent_id: snapshot.run.runtime_agent_id.as_str().to_string(),
        agent_definition_id: snapshot.run.agent_definition_id.clone(),
        agent_definition_version: i64::from(snapshot.run.agent_definition_version),
        system_prompt: snapshot.run.system_prompt.clone(),
        project_id: snapshot.run.project_id.clone(),
        agent_session_id: snapshot.run.agent_session_id.clone(),
        run_id: snapshot.run.run_id.clone(),
        provider_id: snapshot.run.provider_id.clone(),
        model_id: snapshot.run.model_id.clone(),
        status: core_status_from_desktop(&snapshot.run.status),
        prompt: snapshot.run.prompt.clone(),
        messages: snapshot
            .messages
            .into_iter()
            .map(core_message_from_desktop)
            .collect(),
        events: snapshot
            .events
            .into_iter()
            .map(core_event_from_desktop)
            .collect(),
        context_manifests: context_manifests
            .into_iter()
            .map(core_context_manifest_from_desktop)
            .collect(),
    }
}

fn core_message_from_desktop(message: AgentMessageRecord) -> CoreRuntimeMessage {
    CoreRuntimeMessage {
        id: message.id,
        project_id: message.project_id,
        run_id: message.run_id,
        role: core_message_role_from_desktop(&message.role),
        content: message.content,
        provider_metadata: message
            .provider_metadata_json
            .as_deref()
            .and_then(|metadata| serde_json::from_str(metadata).ok()),
        created_at: message.created_at,
    }
}

fn core_event_from_desktop(event: AgentEventRecord) -> CoreRuntimeEvent {
    let payload = serde_json::from_str::<JsonValue>(&event.payload_json).unwrap_or(JsonValue::Null);
    let trace_id = runtime_trace_id_for_run(&event.project_id, &event.run_id);
    let event_kind = core_event_kind_from_desktop(&event.event_kind);
    CoreRuntimeEvent {
        id: event.id,
        project_id: event.project_id,
        run_id: event.run_id.clone(),
        event_kind: event_kind.clone(),
        trace: RuntimeTraceContext::for_event(&trace_id, &event.run_id, event.id, &event_kind),
        payload,
        created_at: event.created_at,
    }
}

fn core_context_manifest_from_desktop(
    manifest: project_store::AgentContextManifestRecord,
) -> CoreContextManifest {
    let project_id = manifest.project_id.clone();
    let run_id = manifest.run_id.clone().unwrap_or_default();
    let manifest_id = manifest.manifest_id.clone();
    let trace_id = runtime_trace_id_for_run(&project_id, &run_id);
    let turn_index = manifest
        .manifest
        .get("turnIndex")
        .and_then(JsonValue::as_u64)
        .unwrap_or_default() as usize;
    CoreContextManifest {
        manifest_id: manifest.manifest_id,
        project_id: manifest.project_id,
        agent_session_id: manifest.agent_session_id,
        run_id: run_id.clone(),
        provider_id: manifest.provider_id.unwrap_or_default(),
        model_id: manifest.model_id.unwrap_or_default(),
        turn_index,
        context_hash: manifest.context_hash,
        recorded_after_event_id: None,
        trace: RuntimeTraceContext::for_context_manifest(
            &trace_id,
            &run_id,
            &manifest_id,
            turn_index,
        ),
        manifest: manifest.manifest,
        created_at: manifest.created_at,
    }
}

fn core_status_from_desktop(status: &AgentRunStatus) -> CoreRunStatus {
    match status {
        AgentRunStatus::Starting => CoreRunStatus::Starting,
        AgentRunStatus::Running => CoreRunStatus::Running,
        AgentRunStatus::Paused => CoreRunStatus::Paused,
        AgentRunStatus::Cancelling => CoreRunStatus::Cancelling,
        AgentRunStatus::Cancelled => CoreRunStatus::Cancelled,
        AgentRunStatus::HandedOff => CoreRunStatus::HandedOff,
        AgentRunStatus::Completed => CoreRunStatus::Completed,
        AgentRunStatus::Failed => CoreRunStatus::Failed,
    }
}

fn core_message_role_from_desktop(role: &AgentMessageRole) -> CoreMessageRole {
    match role {
        AgentMessageRole::System => CoreMessageRole::System,
        AgentMessageRole::Developer => CoreMessageRole::Developer,
        AgentMessageRole::User => CoreMessageRole::User,
        AgentMessageRole::Assistant => CoreMessageRole::Assistant,
        AgentMessageRole::Tool => CoreMessageRole::Tool,
    }
}

fn core_event_kind_from_desktop(kind: &AgentRunEventKind) -> CoreRuntimeEventKind {
    match kind {
        AgentRunEventKind::RunStarted => CoreRuntimeEventKind::RunStarted,
        AgentRunEventKind::MessageDelta => CoreRuntimeEventKind::MessageDelta,
        AgentRunEventKind::ReasoningSummary => CoreRuntimeEventKind::ReasoningSummary,
        AgentRunEventKind::ToolStarted => CoreRuntimeEventKind::ToolStarted,
        AgentRunEventKind::ToolDelta => CoreRuntimeEventKind::ToolDelta,
        AgentRunEventKind::ToolCompleted => CoreRuntimeEventKind::ToolCompleted,
        AgentRunEventKind::FileChanged => CoreRuntimeEventKind::FileChanged,
        AgentRunEventKind::CommandOutput => CoreRuntimeEventKind::CommandOutput,
        AgentRunEventKind::ValidationStarted => CoreRuntimeEventKind::ValidationStarted,
        AgentRunEventKind::ValidationCompleted => CoreRuntimeEventKind::ValidationCompleted,
        AgentRunEventKind::ToolRegistrySnapshot => CoreRuntimeEventKind::ToolRegistrySnapshot,
        AgentRunEventKind::PolicyDecision => CoreRuntimeEventKind::PolicyDecision,
        AgentRunEventKind::StateTransition => CoreRuntimeEventKind::StateTransition,
        AgentRunEventKind::PlanUpdated => CoreRuntimeEventKind::PlanUpdated,
        AgentRunEventKind::VerificationGate => CoreRuntimeEventKind::VerificationGate,
        AgentRunEventKind::ContextManifestRecorded => CoreRuntimeEventKind::ContextManifestRecorded,
        AgentRunEventKind::RetrievalPerformed => CoreRuntimeEventKind::RetrievalPerformed,
        AgentRunEventKind::MemoryCandidateCaptured => CoreRuntimeEventKind::MemoryCandidateCaptured,
        AgentRunEventKind::EnvironmentLifecycleUpdate => {
            CoreRuntimeEventKind::EnvironmentLifecycleUpdate
        }
        AgentRunEventKind::SandboxLifecycleUpdate => CoreRuntimeEventKind::SandboxLifecycleUpdate,
        AgentRunEventKind::ActionRequired => CoreRuntimeEventKind::ActionRequired,
        AgentRunEventKind::ApprovalRequired => CoreRuntimeEventKind::ApprovalRequired,
        AgentRunEventKind::ToolPermissionGrant => CoreRuntimeEventKind::ToolPermissionGrant,
        AgentRunEventKind::ProviderModelChanged => CoreRuntimeEventKind::ProviderModelChanged,
        AgentRunEventKind::RuntimeSettingsChanged => CoreRuntimeEventKind::RuntimeSettingsChanged,
        AgentRunEventKind::RunPaused => CoreRuntimeEventKind::RunPaused,
        AgentRunEventKind::RunCompleted => CoreRuntimeEventKind::RunCompleted,
        AgentRunEventKind::RunFailed => CoreRuntimeEventKind::RunFailed,
        AgentRunEventKind::SubagentLifecycle => CoreRuntimeEventKind::SubagentLifecycle,
    }
}
