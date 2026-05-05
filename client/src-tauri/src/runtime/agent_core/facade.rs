use std::{path::PathBuf, thread};

use serde_json::Value as JsonValue;
use xero_agent_core::{
    runtime_trace_id_for_run, AgentRuntimeFacade, ContextManifest as CoreContextManifest,
    ExportTraceRequest, MessageRole as CoreMessageRole, RunSnapshot as CoreRunSnapshot,
    RunStatus as CoreRunStatus, RuntimeEvent as CoreRuntimeEvent,
    RuntimeEventKind as CoreRuntimeEventKind, RuntimeMessage as CoreRuntimeMessage, RuntimeTrace,
    RuntimeTraceContext,
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
pub struct DesktopExportTraceRequest {
    pub repo_root: PathBuf,
    pub project_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone)]
pub struct DesktopUnsupportedRuntimeRequest;

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
    type RejectRequest = DesktopUnsupportedRuntimeRequest;
    type CancelRunRequest = DesktopCancelRunRequest;
    type ResumeRunRequest = DesktopContinueRunRequest;
    type ForkSessionRequest = DesktopUnsupportedRuntimeRequest;
    type CompactSessionRequest = DesktopUnsupportedRuntimeRequest;
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
        _request: DesktopUnsupportedRuntimeRequest,
    ) -> CommandResult<AgentRunSnapshotRecord> {
        Err(unsupported_desktop_facade_operation("reject_action"))
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
        _request: DesktopUnsupportedRuntimeRequest,
    ) -> CommandResult<AgentRunSnapshotRecord> {
        Err(unsupported_desktop_facade_operation("fork_session"))
    }

    fn compact_session(
        &self,
        _request: DesktopUnsupportedRuntimeRequest,
    ) -> CommandResult<AgentRunSnapshotRecord> {
        Err(unsupported_desktop_facade_operation("compact_session"))
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

fn unsupported_desktop_facade_operation(operation: &str) -> CommandError {
    CommandError::user_fixable(
        "agent_core_operation_unsupported",
        format!("The desktop agent-core adapter does not implement `{operation}` yet."),
    )
}

fn core_snapshot_from_desktop(
    snapshot: AgentRunSnapshotRecord,
    context_manifests: Vec<project_store::AgentContextManifestRecord>,
) -> CoreRunSnapshot {
    CoreRunSnapshot {
        trace_id: runtime_trace_id_for_run(&snapshot.run.project_id, &snapshot.run.run_id),
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
        AgentRunEventKind::ActionRequired => CoreRuntimeEventKind::ActionRequired,
        AgentRunEventKind::RunPaused => CoreRuntimeEventKind::RunPaused,
        AgentRunEventKind::RunCompleted => CoreRuntimeEventKind::RunCompleted,
        AgentRunEventKind::RunFailed => CoreRuntimeEventKind::RunFailed,
    }
}
