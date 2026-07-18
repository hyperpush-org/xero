use std::{
    collections::BTreeSet,
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
            if let Err(error) = self.spawn_owned_agent_run(request) {
                if error.code != "agent_run_already_active" {
                    return Err(error);
                }
            }
        }
        Ok(snapshot)
    }

    pub fn continue_run(
        &self,
        request: ContinueOwnedAgentRunRequest,
        drive_mode: DesktopRunDriveMode,
    ) -> CommandResult<PreparedOwnedAgentContinuation> {
        let source_run = project_store::load_agent_run_record(
            &request.repo_root,
            &request.project_id,
            &request.run_id,
        )?;
        let source_lease = self.supervisor.begin_persisted(
            &request.repo_root,
            &request.project_id,
            &source_run.agent_session_id,
            &request.run_id,
        )?;
        let mut prepared = prepare_owned_agent_continuation_for_drive(&request)?;
        if prepared.drive_required {
            let drive_lease = if prepared.drive_request.run_id == request.run_id {
                source_lease
            } else {
                let target_run = project_store::load_agent_run_record(
                    &prepared.drive_request.repo_root,
                    &prepared.drive_request.project_id,
                    &prepared.drive_request.run_id,
                )?;
                let target_lease = self.supervisor.begin_persisted(
                    &prepared.drive_request.repo_root,
                    &prepared.drive_request.project_id,
                    &target_run.agent_session_id,
                    &prepared.drive_request.run_id,
                )?;
                drop(source_lease);
                target_lease
            };
            prepared.drive_lease = Some(drive_lease);
        } else {
            drop(source_lease);
        }
        if drive_mode == DesktopRunDriveMode::Background && prepared.drive_required {
            let drive_lease = prepared.drive_lease.take();
            self.spawn_owned_agent_continuation(
                prepared.snapshot.run.agent_session_id.clone(),
                prepared.drive_request.clone(),
                drive_lease,
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
        let run_id = resolve_agent_control_leaf(&repo_root, &project_id, &run_id)?;
        let event_payload = serde_json::to_string(&json!({
            "code": AGENT_RUN_CANCELLED_CODE,
            "message": "Owned agent run was cancelled.",
            "state": AgentRunState::Blocked.as_str(),
            "stopReason": AgentRunStopReason::Cancelled.as_str(),
        }))
        .map_err(|error| {
            CommandError::system_fault(
                "agent_run_cancel_event_serialize_failed",
                format!("Xero could not serialize the cancellation event: {error}"),
            )
        })?;
        for _ in 0..4 {
            let expected =
                project_store::load_agent_run_drive_lease(&repo_root, &project_id, &run_id)?;
            let defer_to_drive_owner = expected
                .as_ref()
                .is_some_and(|lease| self.supervisor.persisted_lease_is_foreign_and_live(lease));
            match project_store::cancel_agent_run_with_expected_drive_lease(
                &repo_root,
                &project_id,
                &run_id,
                expected.as_ref(),
                defer_to_drive_owner,
                &event_payload,
                &now_timestamp(),
            )? {
                project_store::AgentRunCancellationCasResult::Applied {
                    snapshot,
                    transitioned: _,
                    event,
                } => {
                    let _ = self.supervisor.cancel(&run_id)?;
                    if let Some(event) = event.as_ref() {
                        publish_committed_agent_event(&repo_root, event);
                    }
                    return Ok(snapshot);
                }
                project_store::AgentRunCancellationCasResult::LeaseChanged(_) => continue,
            }
        }
        Err(CommandError::retryable(
            "agent_run_cancel_raced",
            format!(
                "Xero could not cancel owned-agent run `{run_id}` because its drive ownership kept changing. Retry the cancellation."
            ),
        ))
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

        let committed = project_store::reject_agent_action_and_fail_run(
            &repo_root,
            &request.project_id,
            &request.run_id,
            &request.action_id,
            request.response.as_deref(),
            &now_timestamp(),
        )?;
        for event in &committed.inserted_events {
            publish_committed_agent_event(&repo_root, event);
        }
        Ok(committed.snapshot)
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
        let snapshot = project_store::load_agent_run(
            &request.repo_root,
            &request.project_id,
            &request.run_id,
        )?;
        if snapshot.run.status != AgentRunStatus::Running {
            return Ok(());
        }
        let continuation = initial_owned_agent_continuation_request(&request);
        let registration = register_existing_initial_agent_continuation(&continuation, &snapshot)?;
        let durable_request = if registration.request.state
            == project_store::AgentContinuationRequestState::Driving
        {
            project_store::reconcile_completed_agent_continuation(
                &request.repo_root,
                &request.project_id,
                &request.run_id,
                &continuation.continuation_request_id,
                &now_timestamp(),
            )?
            .ok_or_else(|| {
                CommandError::system_fault(
                    "agent_continuation_request_missing",
                    "The initial agent-start request disappeared during recovery.",
                )
            })?
        } else {
            registration.request
        };
        if durable_request.state != project_store::AgentContinuationRequestState::Prepared {
            return Ok(());
        }
        let lease = self.supervisor.begin_persisted(
            &request.repo_root,
            &request.project_id,
            &request.agent_session_id,
            &request.run_id,
        )?;
        let supervisor = self.supervisor.clone();
        let failure_repo_root = request.repo_root.clone();
        let failure_project_id = request.project_id.clone();
        let failure_run_id = request.run_id.clone();
        thread::spawn(move || {
            let token = lease.token();
            if let Err(error) =
                drive_owned_agent_continuation(continuation, token, Some(supervisor))
            {
                let _ = record_unhandled_owned_agent_drive_error(
                    &failure_repo_root,
                    &failure_project_id,
                    &failure_run_id,
                    &error,
                );
            }
            drop(lease);
        });
        Ok(())
    }

    pub fn spawn_owned_agent_continuation(
        &self,
        agent_session_id: String,
        request: ContinueOwnedAgentRunRequest,
        drive_lease: Option<AgentRunLease>,
    ) -> CommandResult<()> {
        let lease = match drive_lease {
            Some(lease)
                if lease.matches(&request.project_id, &agent_session_id, &request.run_id) =>
            {
                lease
            }
            Some(_) => {
                return Err(CommandError::system_fault(
                    "agent_run_drive_lease_mismatch",
                    "Xero could not start the owned-agent continuation because its prepared drive lease does not match the target run.",
                ));
            }
            None => {
                return Err(CommandError::system_fault(
                    "agent_run_drive_lease_missing",
                    "Xero could not start the owned-agent continuation because its prepared drive lease is missing.",
                ));
            }
        };
        let supervisor = self.supervisor.clone();
        let failure_repo_root = request.repo_root.clone();
        let failure_project_id = request.project_id.clone();
        let failure_run_id = request.run_id.clone();
        thread::spawn(move || {
            let token = lease.token();
            if let Err(error) = drive_owned_agent_continuation(request, token, Some(supervisor)) {
                let _ = record_unhandled_owned_agent_drive_error(
                    &failure_repo_root,
                    &failure_project_id,
                    &failure_run_id,
                    &error,
                );
            }
            drop(lease);
        });
        Ok(())
    }

    pub fn recover_prepared_continuation(
        &self,
        request: ContinueOwnedAgentRunRequest,
    ) -> CommandResult<bool> {
        let prepared = recover_prepared_owned_agent_continuation(&request)?;
        if !prepared.drive_required {
            return Ok(false);
        }
        let lease = match self.supervisor.begin_persisted(
            &request.repo_root,
            &request.project_id,
            &prepared.snapshot.run.agent_session_id,
            &request.run_id,
        ) {
            Ok(lease) => lease,
            Err(error) if error.code == "agent_run_already_active" => return Ok(false),
            Err(error) => return Err(error),
        };
        self.spawn_owned_agent_continuation(
            prepared.snapshot.run.agent_session_id,
            request,
            Some(lease),
        )?;
        Ok(true)
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

fn resolve_agent_control_leaf(
    repo_root: &Path,
    project_id: &str,
    requested_run_id: &str,
) -> CommandResult<String> {
    const MAX_HANDOFF_DEPTH: usize = 32;
    let mut current = requested_run_id.to_owned();
    let mut visited = BTreeSet::new();
    for _ in 0..MAX_HANDOFF_DEPTH {
        if !visited.insert(current.clone()) {
            return Err(CommandError::system_fault(
                "agent_handoff_cycle_detected",
                format!("Xero found a cycle while resolving handed-off run `{requested_run_id}`."),
            ));
        }
        let snapshot = project_store::load_agent_run(repo_root, project_id, &current)?;
        if snapshot.run.status != AgentRunStatus::HandedOff {
            return Ok(current);
        }
        let target =
            project_store::list_agent_handoff_lineage_for_source(repo_root, project_id, &current)?
                .into_iter()
                .find(|lineage| {
                    lineage.status == project_store::AgentHandoffLineageStatus::Completed
                        && lineage.target_run_id.is_some()
                })
                .and_then(|lineage| lineage.target_run_id)
                .ok_or_else(|| {
                    CommandError::system_fault(
                        "agent_handoff_leaf_missing",
                        format!(
                    "Owned-agent run `{current}` is handed off without completed target lineage."
                ),
                    )
                })?;
        current = target;
    }
    Err(CommandError::system_fault(
        "agent_handoff_depth_exceeded",
        format!(
            "Xero refused to follow more than {MAX_HANDOFF_DEPTH} handoffs from run `{requested_run_id}`."
        ),
    ))
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
        AgentRunEventKind::AssistantCandidate => CoreRuntimeEventKind::AssistantCandidate,
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
        AgentRunEventKind::RouteRequested => CoreRuntimeEventKind::RouteRequested,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_session_operation_run_ids_are_validated_without_storage_lookup() {
        assert_eq!(
            run_id_for_session_operation(
                Path::new("/unused"),
                "project",
                "session",
                Some("run-1".into()),
            )
            .expect("explicit run id"),
            "run-1"
        );
        assert_eq!(
            source_run_id_for_session_fork(
                Path::new("/unused"),
                "project",
                "session",
                Some("run-2".into()),
            )
            .expect("explicit source run id"),
            "run-2"
        );
        assert!(run_id_for_session_operation(
            Path::new("/unused"),
            "project",
            "session",
            Some("   ".into()),
        )
        .is_err());
        assert!(source_run_id_for_session_fork(
            Path::new("/unused"),
            "project",
            "session",
            Some(String::new()),
        )
        .is_err());
    }

    #[test]
    fn desktop_export_request_converts_to_core_contract() {
        let request = ExportTraceRequest::from(DesktopExportTraceRequest {
            repo_root: PathBuf::from("/unused"),
            project_id: "project".into(),
            run_id: "run-1".into(),
        });
        assert_eq!(request.project_id, "project");
        assert_eq!(request.run_id, "run-1");
    }

    #[test]
    fn every_desktop_run_status_maps_to_the_core_protocol() {
        for (desktop, core) in [
            (AgentRunStatus::Starting, CoreRunStatus::Starting),
            (AgentRunStatus::Running, CoreRunStatus::Running),
            (AgentRunStatus::Paused, CoreRunStatus::Paused),
            (AgentRunStatus::Cancelling, CoreRunStatus::Cancelling),
            (AgentRunStatus::Cancelled, CoreRunStatus::Cancelled),
            (AgentRunStatus::HandedOff, CoreRunStatus::HandedOff),
            (AgentRunStatus::Completed, CoreRunStatus::Completed),
            (AgentRunStatus::Failed, CoreRunStatus::Failed),
        ] {
            assert_eq!(core_status_from_desktop(&desktop), core);
        }
    }

    #[test]
    fn every_desktop_message_role_maps_to_the_core_protocol() {
        for (desktop, core) in [
            (AgentMessageRole::System, CoreMessageRole::System),
            (AgentMessageRole::Developer, CoreMessageRole::Developer),
            (AgentMessageRole::User, CoreMessageRole::User),
            (AgentMessageRole::Assistant, CoreMessageRole::Assistant),
            (AgentMessageRole::Tool, CoreMessageRole::Tool),
        ] {
            assert_eq!(core_message_role_from_desktop(&desktop), core);
        }
    }

    #[test]
    fn every_desktop_event_kind_maps_to_the_core_protocol() {
        for (desktop, core) in [
            (
                AgentRunEventKind::RunStarted,
                CoreRuntimeEventKind::RunStarted,
            ),
            (
                AgentRunEventKind::AssistantCandidate,
                CoreRuntimeEventKind::AssistantCandidate,
            ),
            (
                AgentRunEventKind::MessageDelta,
                CoreRuntimeEventKind::MessageDelta,
            ),
            (
                AgentRunEventKind::ReasoningSummary,
                CoreRuntimeEventKind::ReasoningSummary,
            ),
            (
                AgentRunEventKind::ToolStarted,
                CoreRuntimeEventKind::ToolStarted,
            ),
            (
                AgentRunEventKind::ToolDelta,
                CoreRuntimeEventKind::ToolDelta,
            ),
            (
                AgentRunEventKind::ToolCompleted,
                CoreRuntimeEventKind::ToolCompleted,
            ),
            (
                AgentRunEventKind::FileChanged,
                CoreRuntimeEventKind::FileChanged,
            ),
            (
                AgentRunEventKind::CommandOutput,
                CoreRuntimeEventKind::CommandOutput,
            ),
            (
                AgentRunEventKind::ValidationStarted,
                CoreRuntimeEventKind::ValidationStarted,
            ),
            (
                AgentRunEventKind::ValidationCompleted,
                CoreRuntimeEventKind::ValidationCompleted,
            ),
            (
                AgentRunEventKind::ToolRegistrySnapshot,
                CoreRuntimeEventKind::ToolRegistrySnapshot,
            ),
            (
                AgentRunEventKind::PolicyDecision,
                CoreRuntimeEventKind::PolicyDecision,
            ),
            (
                AgentRunEventKind::StateTransition,
                CoreRuntimeEventKind::StateTransition,
            ),
            (
                AgentRunEventKind::PlanUpdated,
                CoreRuntimeEventKind::PlanUpdated,
            ),
            (
                AgentRunEventKind::RouteRequested,
                CoreRuntimeEventKind::RouteRequested,
            ),
            (
                AgentRunEventKind::VerificationGate,
                CoreRuntimeEventKind::VerificationGate,
            ),
            (
                AgentRunEventKind::ContextManifestRecorded,
                CoreRuntimeEventKind::ContextManifestRecorded,
            ),
            (
                AgentRunEventKind::RetrievalPerformed,
                CoreRuntimeEventKind::RetrievalPerformed,
            ),
            (
                AgentRunEventKind::MemoryCandidateCaptured,
                CoreRuntimeEventKind::MemoryCandidateCaptured,
            ),
            (
                AgentRunEventKind::EnvironmentLifecycleUpdate,
                CoreRuntimeEventKind::EnvironmentLifecycleUpdate,
            ),
            (
                AgentRunEventKind::SandboxLifecycleUpdate,
                CoreRuntimeEventKind::SandboxLifecycleUpdate,
            ),
            (
                AgentRunEventKind::ActionRequired,
                CoreRuntimeEventKind::ActionRequired,
            ),
            (
                AgentRunEventKind::ApprovalRequired,
                CoreRuntimeEventKind::ApprovalRequired,
            ),
            (
                AgentRunEventKind::ToolPermissionGrant,
                CoreRuntimeEventKind::ToolPermissionGrant,
            ),
            (
                AgentRunEventKind::ProviderModelChanged,
                CoreRuntimeEventKind::ProviderModelChanged,
            ),
            (
                AgentRunEventKind::RuntimeSettingsChanged,
                CoreRuntimeEventKind::RuntimeSettingsChanged,
            ),
            (
                AgentRunEventKind::RunPaused,
                CoreRuntimeEventKind::RunPaused,
            ),
            (
                AgentRunEventKind::RunCompleted,
                CoreRuntimeEventKind::RunCompleted,
            ),
            (
                AgentRunEventKind::RunFailed,
                CoreRuntimeEventKind::RunFailed,
            ),
            (
                AgentRunEventKind::SubagentLifecycle,
                CoreRuntimeEventKind::SubagentLifecycle,
            ),
        ] {
            assert_eq!(core_event_kind_from_desktop(&desktop), core);
        }
    }

    #[test]
    fn message_conversion_decodes_valid_metadata_and_discards_malformed_metadata() {
        let message = |metadata: &str| AgentMessageRecord {
            id: 1,
            project_id: "project".into(),
            run_id: "run-1".into(),
            role: AgentMessageRole::Assistant,
            content: "done".into(),
            provider_metadata_json: Some(metadata.into()),
            created_at: "2026-07-17T00:00:00Z".into(),
            attachments: Vec::new(),
        };

        let converted = core_message_from_desktop(message(r#"{"providerMessageId":"msg-1"}"#));
        assert_eq!(converted.role, CoreMessageRole::Assistant);
        assert_eq!(
            converted
                .provider_metadata
                .as_ref()
                .and_then(|metadata| metadata.provider_message_id.as_deref()),
            Some("msg-1")
        );
        assert_eq!(
            core_message_from_desktop(message("not-json")).provider_metadata,
            None
        );
    }

    #[test]
    fn event_conversion_preserves_valid_payload_and_nulls_malformed_payload() {
        let event = |payload_json: &str| AgentEventRecord {
            id: 4,
            project_id: "project".into(),
            run_id: "run-1".into(),
            event_kind: AgentRunEventKind::PolicyDecision,
            payload_json: payload_json.into(),
            created_at: "2026-07-17T00:00:00Z".into(),
        };

        let converted = core_event_from_desktop(event(r#"{"allowed":true}"#));
        assert_eq!(converted.event_kind, CoreRuntimeEventKind::PolicyDecision);
        assert_eq!(converted.payload, json!({ "allowed": true }));
        assert_eq!(
            core_event_from_desktop(event("not-json")).payload,
            JsonValue::Null
        );
    }

    #[test]
    fn context_manifest_conversion_supplies_defaults_and_turn_index() {
        let converted =
            core_context_manifest_from_desktop(project_store::AgentContextManifestRecord {
                id: 1,
                manifest_id: "manifest-1".into(),
                project_id: "project".into(),
                agent_session_id: "session".into(),
                run_id: None,
                runtime_agent_id: crate::commands::RuntimeAgentIdDto::Engineer,
                agent_definition_id: "engineer".into(),
                agent_definition_version: 1,
                provider_id: None,
                model_id: None,
                request_kind: project_store::AgentContextManifestRequestKind::Test,
                policy_action: project_store::AgentContextPolicyAction::ContinueNow,
                policy_reason_code: "test".into(),
                budget_tokens: None,
                estimated_tokens: 1,
                pressure: project_store::AgentContextBudgetPressure::Low,
                context_hash: "hash".into(),
                included_contributors: Vec::new(),
                excluded_contributors: Vec::new(),
                retrieval_query_ids: Vec::new(),
                retrieval_result_ids: Vec::new(),
                compaction_id: None,
                handoff_id: None,
                redaction_state: project_store::AgentContextRedactionState::Clean,
                manifest: json!({ "turnIndex": 3 }),
                created_at: "2026-07-17T00:00:00Z".into(),
            });

        assert_eq!(converted.run_id, "");
        assert_eq!(converted.provider_id, "");
        assert_eq!(converted.model_id, "");
        assert_eq!(converted.turn_index, 3);
        assert_eq!(converted.context_hash, "hash");
    }
}
