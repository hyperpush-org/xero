use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use rand::RngCore;
use tauri::{AppHandle, Emitter, Runtime};

use crate::{
    auth::{AuthDiagnostic, AuthFlowError},
    commands::{
        AutonomousArtifactPayloadDto, AutonomousCommandResultDto, AutonomousLifecycleReasonDto,
        AutonomousPolicyDeniedPayloadDto, AutonomousRunDto, AutonomousRunRecoveryStateDto,
        AutonomousRunStateDto, AutonomousRunStatusDto, AutonomousSkillCacheStatusDto,
        AutonomousSkillLifecycleCacheDto, AutonomousSkillLifecycleDiagnosticDto,
        AutonomousSkillLifecyclePayloadDto, AutonomousSkillLifecycleResultDto,
        AutonomousSkillLifecycleSourceDto, AutonomousSkillLifecycleStageDto,
        AutonomousToolCallStateDto, AutonomousToolResultPayloadDto, AutonomousUnitArtifactDto,
        AutonomousUnitArtifactStatusDto, AutonomousUnitAttemptDto, AutonomousUnitDto,
        AutonomousUnitHistoryEntryDto, AutonomousUnitKindDto, AutonomousUnitStatusDto,
        AutonomousVerificationEvidencePayloadDto, AutonomousVerificationOutcomeDto,
        AutonomousWorkflowLinkageDto, CommandError, CommandErrorClass, CommandResult,
        CommandToolResultSummaryDto, FileToolResultSummaryDto, GitToolResultScopeDto,
        GitToolResultSummaryDto, ProjectUpdateReason, ProjectUpdatedPayloadDto,
        RuntimeDiagnosticDto, RuntimeRunCheckpointDto, RuntimeRunCheckpointKindDto,
        RuntimeRunDiagnosticDto, RuntimeRunDto, RuntimeRunStatusDto, RuntimeRunTransportDto,
        RuntimeRunTransportLivenessDto, RuntimeRunUpdatedPayloadDto, RuntimeSessionDto,
        RuntimeUpdatedPayloadDto, ToolResultSummaryDto, WebToolResultContentKindDto,
        WebToolResultSummaryDto, PROJECT_UPDATED_EVENT, RUNTIME_RUN_UPDATED_EVENT,
        RUNTIME_UPDATED_EVENT,
    },
    db::project_store::{
        self, AutonomousArtifactCommandResultRecord, AutonomousArtifactPayloadRecord,
        AutonomousRunSnapshotRecord, AutonomousRunStatus, AutonomousRunUpsertRecord,
        AutonomousSkillCacheStatusRecord, AutonomousSkillLifecycleDiagnosticRecord,
        AutonomousSkillLifecycleResultRecord, AutonomousSkillLifecycleSourceRecord,
        AutonomousSkillLifecycleStageRecord, AutonomousToolCallStateRecord,
        AutonomousUnitArtifactRecord, AutonomousUnitArtifactStatus, AutonomousUnitAttemptRecord,
        AutonomousUnitHistoryRecord, AutonomousUnitKind, AutonomousUnitRecord,
        AutonomousUnitStatus, AutonomousVerificationOutcomeRecord, RuntimeRunCheckpointKind,
        RuntimeRunDiagnosticRecord, RuntimeRunSnapshotRecord, RuntimeRunStatus,
        RuntimeRunTransportLiveness, RuntimeSessionDiagnosticRecord, RuntimeSessionRecord,
    },
    runtime::{
        autonomous_orchestrator::{reconcile_runtime_snapshot, AutonomousRuntimeReconcileIntent},
        autonomous_workflow_progression::persist_autonomous_workflow_progression,
        default_runtime_provider, probe_runtime_run,
        protocol::{
            GitToolResultScope, SupervisorSkillCacheStatus, SupervisorSkillDiagnostic,
            SupervisorSkillLifecycleResult, SupervisorSkillLifecycleStage,
            SupervisorSkillSourceMetadata, ToolResultSummary, WebToolResultContentKind,
        },
        resolve_runtime_provider_identity, RuntimeSupervisorProbeRequest,
    },
    state::DesktopState,
};

pub(crate) const DEFAULT_RUNTIME_RUN_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT: Duration = Duration::from_millis(750);
pub(crate) const DEFAULT_RUNTIME_RUN_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(4);

pub(crate) fn resolve_project_root<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
) -> CommandResult<PathBuf> {
    crate::runtime::resolve_imported_repo_root(app, state, project_id)
}

pub(crate) fn default_runtime_session(project_id: &str) -> RuntimeSessionDto {
    let provider = default_runtime_provider();

    RuntimeSessionDto {
        project_id: project_id.into(),
        runtime_kind: provider.runtime_kind.into(),
        provider_id: provider.provider_id.into(),
        flow_id: None,
        session_id: None,
        account_id: None,
        phase: crate::commands::RuntimeAuthPhase::Idle,
        callback_bound: None,
        authorization_url: None,
        redirect_uri: None,
        last_error_code: None,
        last_error: None,
        updated_at: crate::auth::now_timestamp(),
    }
}

pub(crate) fn load_runtime_session_status(
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<RuntimeSessionDto> {
    let stored = project_store::load_runtime_session(repo_root, project_id)?;
    Ok(runtime_session_from_record(
        state,
        project_id,
        stored.as_ref(),
    ))
}

pub(crate) fn persist_runtime_session(
    repo_root: &Path,
    runtime: &RuntimeSessionDto,
) -> CommandResult<RuntimeSessionDto> {
    let record = RuntimeSessionRecord {
        project_id: runtime.project_id.clone(),
        runtime_kind: runtime.runtime_kind.clone(),
        provider_id: runtime.provider_id.clone(),
        flow_id: runtime.flow_id.clone(),
        session_id: runtime.session_id.clone(),
        account_id: runtime.account_id.clone(),
        auth_phase: runtime.phase.clone(),
        last_error: runtime
            .last_error
            .as_ref()
            .map(|error| RuntimeSessionDiagnosticRecord {
                code: error.code.clone(),
                message: error.message.clone(),
                retryable: error.retryable,
            }),
        updated_at: runtime.updated_at.clone(),
    };
    let persisted = project_store::upsert_runtime_session(repo_root, &record)?;
    Ok(RuntimeSessionDto {
        project_id: persisted.project_id,
        runtime_kind: persisted.runtime_kind,
        provider_id: persisted.provider_id,
        flow_id: persisted.flow_id,
        session_id: persisted.session_id,
        account_id: persisted.account_id,
        phase: persisted.auth_phase,
        callback_bound: None,
        authorization_url: None,
        redirect_uri: None,
        last_error_code: persisted
            .last_error
            .as_ref()
            .map(|error| error.code.clone()),
        last_error: persisted.last_error.map(runtime_diagnostic_from_record),
        updated_at: persisted.updated_at,
    })
}

pub(crate) fn runtime_session_from_record(
    state: &DesktopState,
    project_id: &str,
    stored: Option<&RuntimeSessionRecord>,
) -> RuntimeSessionDto {
    let Some(stored) = stored else {
        return default_runtime_session(project_id);
    };

    let active_flow = stored
        .flow_id
        .as_deref()
        .and_then(|flow_id| state.active_auth_flows().snapshot(flow_id));

    if let Some(flow) = active_flow {
        let last_error = flow.last_error.map(runtime_diagnostic_from_auth);
        return RuntimeSessionDto {
            project_id: stored.project_id.clone(),
            runtime_kind: stored.runtime_kind.clone(),
            provider_id: stored.provider_id.clone(),
            flow_id: Some(flow.flow_id),
            session_id: flow.session_id.or_else(|| stored.session_id.clone()),
            account_id: flow.account_id.or_else(|| stored.account_id.clone()),
            phase: flow.phase,
            callback_bound: Some(flow.callback_bound),
            authorization_url: Some(flow.authorization_url),
            redirect_uri: Some(flow.redirect_uri),
            last_error_code: last_error.as_ref().map(|error| error.code.clone()),
            last_error,
            updated_at: flow.updated_at,
        };
    }

    RuntimeSessionDto {
        project_id: stored.project_id.clone(),
        runtime_kind: stored.runtime_kind.clone(),
        provider_id: stored.provider_id.clone(),
        flow_id: stored.flow_id.clone(),
        session_id: stored.session_id.clone(),
        account_id: stored.account_id.clone(),
        phase: stored.auth_phase.clone(),
        callback_bound: None,
        authorization_url: None,
        redirect_uri: None,
        last_error_code: stored.last_error.as_ref().map(|error| error.code.clone()),
        last_error: stored
            .last_error
            .clone()
            .map(runtime_diagnostic_from_record),
        updated_at: stored.updated_at.clone(),
    }
}

pub(crate) fn emit_runtime_updated<R: Runtime>(
    app: &AppHandle<R>,
    runtime: &RuntimeSessionDto,
) -> CommandResult<()> {
    app.emit(
        RUNTIME_UPDATED_EVENT,
        RuntimeUpdatedPayloadDto {
            project_id: runtime.project_id.clone(),
            runtime_kind: runtime.runtime_kind.clone(),
            provider_id: runtime.provider_id.clone(),
            flow_id: runtime.flow_id.clone(),
            session_id: runtime.session_id.clone(),
            account_id: runtime.account_id.clone(),
            auth_phase: runtime.phase.clone(),
            last_error_code: runtime.last_error_code.clone(),
            last_error: runtime.last_error.clone(),
            updated_at: runtime.updated_at.clone(),
        },
    )
    .map_err(|error| {
        CommandError::retryable(
            "runtime_updated_emit_failed",
            format!(
                "Cadence updated runtime-session metadata but could not emit the runtime update event: {error}"
            ),
        )
    })
}

pub(crate) fn emit_project_updated<R: Runtime>(
    app: &AppHandle<R>,
    repo_root: &Path,
    project_id: &str,
    reason: ProjectUpdateReason,
) -> CommandResult<()> {
    let project = project_store::load_project_summary(repo_root, project_id)?;

    app.emit(
        PROJECT_UPDATED_EVENT,
        ProjectUpdatedPayloadDto { project, reason },
    )
    .map_err(|error| {
        CommandError::retryable(
            "project_updated_emit_failed",
            format!(
                "Cadence updated selected-project metadata but could not emit the project update event: {error}"
            ),
        )
    })
}

pub(crate) fn load_persisted_runtime_run(
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<Option<RuntimeRunSnapshotRecord>> {
    project_store::load_runtime_run(repo_root, project_id)
}

pub(crate) fn load_runtime_run_status(
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<Option<RuntimeRunSnapshotRecord>> {
    probe_runtime_run(
        state,
        RuntimeSupervisorProbeRequest {
            project_id: project_id.into(),
            repo_root: repo_root.to_path_buf(),
            control_timeout: DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT,
        },
    )
}

fn provider_id_from_runtime_kind(runtime_kind: &str) -> String {
    resolve_runtime_provider_identity(Some(runtime_kind), Some(runtime_kind))
        .map(|provider| provider.provider_id.to_string())
        .unwrap_or_else(|_| runtime_kind.trim().to_string())
}

pub(crate) fn runtime_run_dto_from_snapshot(snapshot: &RuntimeRunSnapshotRecord) -> RuntimeRunDto {
    RuntimeRunDto {
        project_id: snapshot.run.project_id.clone(),
        run_id: snapshot.run.run_id.clone(),
        runtime_kind: snapshot.run.runtime_kind.clone(),
        provider_id: provider_id_from_runtime_kind(&snapshot.run.runtime_kind),
        supervisor_kind: snapshot.run.supervisor_kind.clone(),
        status: runtime_run_status_dto(snapshot.run.status.clone()),
        transport: RuntimeRunTransportDto {
            kind: snapshot.run.transport.kind.clone(),
            endpoint: snapshot.run.transport.endpoint.clone(),
            liveness: runtime_run_transport_liveness_dto(snapshot.run.transport.liveness.clone()),
        },
        started_at: snapshot.run.started_at.clone(),
        last_heartbeat_at: snapshot.run.last_heartbeat_at.clone(),
        last_checkpoint_sequence: snapshot.last_checkpoint_sequence,
        last_checkpoint_at: snapshot.last_checkpoint_at.clone(),
        stopped_at: snapshot.run.stopped_at.clone(),
        last_error_code: snapshot
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.clone()),
        last_error: snapshot
            .run
            .last_error
            .as_ref()
            .map(|error| RuntimeRunDiagnosticDto {
                code: error.code.clone(),
                message: error.message.clone(),
            }),
        updated_at: snapshot.run.updated_at.clone(),
        checkpoints: snapshot
            .checkpoints
            .iter()
            .map(|checkpoint| RuntimeRunCheckpointDto {
                sequence: checkpoint.sequence,
                kind: runtime_run_checkpoint_kind_dto(checkpoint.kind.clone()),
                summary: checkpoint.summary.clone(),
                created_at: checkpoint.created_at.clone(),
            })
            .collect(),
    }
}

pub(crate) fn load_persisted_autonomous_run(
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<Option<AutonomousRunSnapshotRecord>> {
    project_store::load_autonomous_run(repo_root, project_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutonomousSyncIntent {
    Observe,
    DuplicateStart,
    CancelRequested,
}

pub(crate) fn sync_autonomous_run_state(
    repo_root: &Path,
    project_id: &str,
    runtime_snapshot: Option<&RuntimeRunSnapshotRecord>,
    intent: AutonomousSyncIntent,
) -> CommandResult<AutonomousRunStateDto> {
    let existing = load_persisted_autonomous_run(repo_root, project_id)?;

    let persisted = match runtime_snapshot {
        Some(snapshot) => {
            let payload = reconcile_autonomous_run_snapshot(existing.as_ref(), snapshot, intent);
            match persist_autonomous_workflow_progression(
                repo_root,
                project_id,
                existing.as_ref(),
                payload.clone(),
            ) {
                Ok(persisted) => persisted,
                Err(error) if should_fallback_autonomous_sync(&error, intent) => {
                    return Ok(autonomous_run_state_from_transient_payload(
                        existing.as_ref(),
                        &payload,
                    ));
                }
                Err(error) => return Err(error),
            }
        }
        None => {
            if let Some(existing) = existing {
                existing
            } else {
                return Ok(AutonomousRunStateDto {
                    run: None,
                    unit: None,
                    attempt: None,
                    history: Vec::new(),
                });
            }
        }
    };

    Ok(autonomous_run_state_from_snapshot(Some(&persisted)))
}

fn should_fallback_autonomous_sync(error: &CommandError, intent: AutonomousSyncIntent) -> bool {
    matches!(
        intent,
        AutonomousSyncIntent::Observe | AutonomousSyncIntent::DuplicateStart
    ) && matches!(error.class, CommandErrorClass::Retryable)
        && matches!(
            error.code.as_str(),
            "autonomous_run_transaction_failed"
                | "autonomous_run_persist_failed"
                | "autonomous_run_commit_failed"
        )
}

fn autonomous_run_state_from_transient_payload(
    existing: Option<&AutonomousRunSnapshotRecord>,
    payload: &AutonomousRunUpsertRecord,
) -> AutonomousRunStateDto {
    let history = merge_transient_autonomous_history(existing, payload);
    let snapshot = AutonomousRunSnapshotRecord {
        run: payload.run.clone(),
        unit: payload.unit.clone(),
        attempt: payload.attempt.clone(),
        history,
    };
    autonomous_run_state_from_snapshot(Some(&snapshot))
}

fn merge_transient_autonomous_history(
    existing: Option<&AutonomousRunSnapshotRecord>,
    payload: &AutonomousRunUpsertRecord,
) -> Vec<AutonomousUnitHistoryRecord> {
    let same_run = existing.is_some_and(|snapshot| snapshot.run.run_id == payload.run.run_id);
    let mut history = if same_run {
        existing
            .map(|snapshot| snapshot.history.clone())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let Some(unit) = payload.unit.as_ref() else {
        return history;
    };

    let next_entry = AutonomousUnitHistoryRecord {
        unit: unit.clone(),
        latest_attempt: payload.attempt.clone(),
        artifacts: payload.artifacts.clone(),
    };

    if let Some(position) = history
        .iter()
        .position(|entry| entry.unit.unit_id == unit.unit_id)
    {
        history[position] = next_entry;
    } else {
        history.insert(0, next_entry);
    }

    history.sort_by(|left, right| right.unit.sequence.cmp(&left.unit.sequence));
    history
}

pub(crate) fn autonomous_run_state_from_snapshot(
    snapshot: Option<&AutonomousRunSnapshotRecord>,
) -> AutonomousRunStateDto {
    AutonomousRunStateDto {
        run: snapshot.map(autonomous_run_dto_from_snapshot),
        unit: snapshot.and_then(autonomous_unit_dto_from_snapshot),
        attempt: snapshot.and_then(autonomous_attempt_dto_from_snapshot),
        history: snapshot
            .map(autonomous_history_dto_from_snapshot)
            .unwrap_or_default(),
    }
}

pub(crate) fn emit_runtime_run_updated<R: Runtime>(
    app: &AppHandle<R>,
    runtime_run: Option<&RuntimeRunDto>,
) -> CommandResult<()> {
    let project_id = runtime_run
        .map(|runtime_run| runtime_run.project_id.clone())
        .unwrap_or_default();

    app.emit(
        RUNTIME_RUN_UPDATED_EVENT,
        RuntimeRunUpdatedPayloadDto {
            project_id,
            run: runtime_run.cloned(),
        },
    )
    .map_err(|error| {
        CommandError::retryable(
            "runtime_run_updated_emit_failed",
            format!(
                "Cadence updated durable runtime-run metadata but could not emit the runtime-run update event: {error}"
            ),
        )
    })
}

pub(crate) fn emit_runtime_run_updated_if_changed<R: Runtime>(
    app: &AppHandle<R>,
    project_id: &str,
    before: &Option<RuntimeRunSnapshotRecord>,
    after: &Option<RuntimeRunSnapshotRecord>,
) -> CommandResult<()> {
    if before == after {
        return Ok(());
    }

    let runtime_run = after.as_ref().map(runtime_run_dto_from_snapshot);
    if let Some(runtime_run) = runtime_run.as_ref() {
        return emit_runtime_run_updated(app, Some(runtime_run));
    }

    app.emit(
        RUNTIME_RUN_UPDATED_EVENT,
        RuntimeRunUpdatedPayloadDto {
            project_id: project_id.into(),
            run: None,
        },
    )
    .map_err(|error| {
        CommandError::retryable(
            "runtime_run_updated_emit_failed",
            format!(
                "Cadence updated durable runtime-run metadata but could not emit the runtime-run update event: {error}"
            ),
        )
    })
}

fn reconcile_autonomous_run_snapshot(
    existing: Option<&AutonomousRunSnapshotRecord>,
    runtime_snapshot: &RuntimeRunSnapshotRecord,
    intent: AutonomousSyncIntent,
) -> AutonomousRunUpsertRecord {
    reconcile_runtime_snapshot(
        existing,
        runtime_snapshot,
        match intent {
            AutonomousSyncIntent::Observe => AutonomousRuntimeReconcileIntent::Observe,
            AutonomousSyncIntent::DuplicateStart => {
                AutonomousRuntimeReconcileIntent::DuplicateStart
            }
            AutonomousSyncIntent::CancelRequested => {
                AutonomousRuntimeReconcileIntent::CancelRequested
            }
        },
    )
}

fn autonomous_run_dto_from_snapshot(snapshot: &AutonomousRunSnapshotRecord) -> AutonomousRunDto {
    AutonomousRunDto {
        project_id: snapshot.run.project_id.clone(),
        run_id: snapshot.run.run_id.clone(),
        runtime_kind: snapshot.run.runtime_kind.clone(),
        provider_id: provider_id_from_runtime_kind(&snapshot.run.runtime_kind),
        supervisor_kind: snapshot.run.supervisor_kind.clone(),
        status: autonomous_run_status_dto(&snapshot.run.status),
        recovery_state: autonomous_run_recovery_state_dto(&snapshot.run.status),
        active_unit_id: snapshot.unit.as_ref().map(|unit| unit.unit_id.clone()),
        active_attempt_id: snapshot
            .attempt
            .as_ref()
            .map(|attempt| attempt.attempt_id.clone()),
        duplicate_start_detected: snapshot.run.duplicate_start_detected,
        duplicate_start_run_id: snapshot.run.duplicate_start_run_id.clone(),
        duplicate_start_reason: snapshot.run.duplicate_start_reason.clone(),
        started_at: snapshot.run.started_at.clone(),
        last_heartbeat_at: snapshot.run.last_heartbeat_at.clone(),
        last_checkpoint_at: snapshot.run.last_checkpoint_at.clone(),
        paused_at: snapshot.run.paused_at.clone(),
        cancelled_at: snapshot.run.cancelled_at.clone(),
        completed_at: snapshot.run.completed_at.clone(),
        crashed_at: snapshot.run.crashed_at.clone(),
        stopped_at: snapshot.run.stopped_at.clone(),
        pause_reason: snapshot.run.pause_reason.as_ref().map(runtime_reason_dto),
        cancel_reason: snapshot.run.cancel_reason.as_ref().map(runtime_reason_dto),
        crash_reason: snapshot.run.crash_reason.as_ref().map(runtime_reason_dto),
        last_error_code: snapshot
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.clone()),
        last_error: snapshot
            .run
            .last_error
            .as_ref()
            .map(runtime_run_diagnostic_dto),
        updated_at: snapshot.run.updated_at.clone(),
    }
}

fn autonomous_unit_dto_from_snapshot(
    snapshot: &AutonomousRunSnapshotRecord,
) -> Option<AutonomousUnitDto> {
    snapshot.unit.as_ref().map(autonomous_unit_dto_from_record)
}

fn autonomous_attempt_dto_from_snapshot(
    snapshot: &AutonomousRunSnapshotRecord,
) -> Option<AutonomousUnitAttemptDto> {
    snapshot
        .attempt
        .as_ref()
        .map(autonomous_attempt_dto_from_record)
}

fn autonomous_history_dto_from_snapshot(
    snapshot: &AutonomousRunSnapshotRecord,
) -> Vec<AutonomousUnitHistoryEntryDto> {
    snapshot
        .history
        .iter()
        .cloned()
        .map(autonomous_history_entry_dto_from_record)
        .collect()
}

fn autonomous_workflow_linkage_dto_from_record(
    linkage: &project_store::AutonomousWorkflowLinkageRecord,
) -> AutonomousWorkflowLinkageDto {
    AutonomousWorkflowLinkageDto {
        workflow_node_id: linkage.workflow_node_id.clone(),
        transition_id: linkage.transition_id.clone(),
        causal_transition_id: linkage.causal_transition_id.clone(),
        handoff_transition_id: linkage.handoff_transition_id.clone(),
        handoff_package_hash: linkage.handoff_package_hash.clone(),
    }
}

fn autonomous_unit_dto_from_record(unit: &AutonomousUnitRecord) -> AutonomousUnitDto {
    AutonomousUnitDto {
        project_id: unit.project_id.clone(),
        run_id: unit.run_id.clone(),
        unit_id: unit.unit_id.clone(),
        sequence: unit.sequence,
        kind: autonomous_unit_kind_dto(&unit.kind),
        status: autonomous_unit_status_dto(&unit.status),
        summary: unit.summary.clone(),
        boundary_id: unit.boundary_id.clone(),
        workflow_linkage: unit
            .workflow_linkage
            .as_ref()
            .map(autonomous_workflow_linkage_dto_from_record),
        started_at: unit.started_at.clone(),
        finished_at: unit.finished_at.clone(),
        updated_at: unit.updated_at.clone(),
        last_error_code: unit.last_error.as_ref().map(|error| error.code.clone()),
        last_error: unit.last_error.as_ref().map(runtime_run_diagnostic_dto),
    }
}

fn autonomous_attempt_dto_from_record(
    attempt: &AutonomousUnitAttemptRecord,
) -> AutonomousUnitAttemptDto {
    AutonomousUnitAttemptDto {
        project_id: attempt.project_id.clone(),
        run_id: attempt.run_id.clone(),
        unit_id: attempt.unit_id.clone(),
        attempt_id: attempt.attempt_id.clone(),
        attempt_number: attempt.attempt_number,
        child_session_id: attempt.child_session_id.clone(),
        status: autonomous_unit_status_dto(&attempt.status),
        boundary_id: attempt.boundary_id.clone(),
        workflow_linkage: attempt
            .workflow_linkage
            .as_ref()
            .map(autonomous_workflow_linkage_dto_from_record),
        started_at: attempt.started_at.clone(),
        finished_at: attempt.finished_at.clone(),
        updated_at: attempt.updated_at.clone(),
        last_error_code: attempt.last_error.as_ref().map(|error| error.code.clone()),
        last_error: attempt.last_error.as_ref().map(runtime_run_diagnostic_dto),
    }
}

fn autonomous_artifact_dto_from_record(
    artifact: &AutonomousUnitArtifactRecord,
) -> AutonomousUnitArtifactDto {
    AutonomousUnitArtifactDto {
        project_id: artifact.project_id.clone(),
        run_id: artifact.run_id.clone(),
        unit_id: artifact.unit_id.clone(),
        attempt_id: artifact.attempt_id.clone(),
        artifact_id: artifact.artifact_id.clone(),
        artifact_kind: artifact.artifact_kind.clone(),
        status: autonomous_artifact_status_dto(&artifact.status),
        summary: artifact.summary.clone(),
        content_hash: artifact.content_hash.clone(),
        payload: artifact
            .payload
            .as_ref()
            .map(autonomous_artifact_payload_dto_from_record),
        created_at: artifact.created_at.clone(),
        updated_at: artifact.updated_at.clone(),
    }
}

fn autonomous_artifact_payload_dto_from_record(
    payload: &AutonomousArtifactPayloadRecord,
) -> AutonomousArtifactPayloadDto {
    match payload {
        AutonomousArtifactPayloadRecord::ToolResult(tool) => {
            AutonomousArtifactPayloadDto::ToolResult(AutonomousToolResultPayloadDto {
                project_id: tool.project_id.clone(),
                run_id: tool.run_id.clone(),
                unit_id: tool.unit_id.clone(),
                attempt_id: tool.attempt_id.clone(),
                artifact_id: tool.artifact_id.clone(),
                tool_call_id: tool.tool_call_id.clone(),
                tool_name: tool.tool_name.clone(),
                tool_state: autonomous_tool_call_state_dto(&tool.tool_state),
                command_result: tool
                    .command_result
                    .as_ref()
                    .map(autonomous_command_result_dto_from_record),
                tool_summary: tool
                    .tool_summary
                    .as_ref()
                    .map(tool_result_summary_dto_from_protocol),
                action_id: tool.action_id.clone(),
                boundary_id: tool.boundary_id.clone(),
            })
        }
        AutonomousArtifactPayloadRecord::VerificationEvidence(evidence) => {
            AutonomousArtifactPayloadDto::VerificationEvidence(
                AutonomousVerificationEvidencePayloadDto {
                    project_id: evidence.project_id.clone(),
                    run_id: evidence.run_id.clone(),
                    unit_id: evidence.unit_id.clone(),
                    attempt_id: evidence.attempt_id.clone(),
                    artifact_id: evidence.artifact_id.clone(),
                    evidence_kind: evidence.evidence_kind.clone(),
                    label: evidence.label.clone(),
                    outcome: autonomous_verification_outcome_dto(&evidence.outcome),
                    command_result: evidence
                        .command_result
                        .as_ref()
                        .map(autonomous_command_result_dto_from_record),
                    action_id: evidence.action_id.clone(),
                    boundary_id: evidence.boundary_id.clone(),
                },
            )
        }
        AutonomousArtifactPayloadRecord::PolicyDenied(policy) => {
            AutonomousArtifactPayloadDto::PolicyDenied(AutonomousPolicyDeniedPayloadDto {
                project_id: policy.project_id.clone(),
                run_id: policy.run_id.clone(),
                unit_id: policy.unit_id.clone(),
                attempt_id: policy.attempt_id.clone(),
                artifact_id: policy.artifact_id.clone(),
                diagnostic_code: policy.diagnostic_code.clone(),
                message: policy.message.clone(),
                tool_name: policy.tool_name.clone(),
                action_id: policy.action_id.clone(),
                boundary_id: policy.boundary_id.clone(),
            })
        }
        AutonomousArtifactPayloadRecord::SkillLifecycle(skill) => {
            AutonomousArtifactPayloadDto::SkillLifecycle(AutonomousSkillLifecyclePayloadDto {
                project_id: skill.project_id.clone(),
                run_id: skill.run_id.clone(),
                unit_id: skill.unit_id.clone(),
                attempt_id: skill.attempt_id.clone(),
                artifact_id: skill.artifact_id.clone(),
                stage: autonomous_skill_lifecycle_stage_dto(&skill.stage),
                result: autonomous_skill_lifecycle_result_dto(&skill.result),
                skill_id: skill.skill_id.clone(),
                source: autonomous_skill_lifecycle_source_dto_from_record(&skill.source),
                cache: autonomous_skill_lifecycle_cache_dto_from_record(&skill.cache),
                diagnostic: skill
                    .diagnostic
                    .as_ref()
                    .map(autonomous_skill_lifecycle_diagnostic_dto_from_record),
            })
        }
    }
}

fn autonomous_command_result_dto_from_record(
    command_result: &AutonomousArtifactCommandResultRecord,
) -> AutonomousCommandResultDto {
    AutonomousCommandResultDto {
        exit_code: command_result.exit_code,
        timed_out: command_result.timed_out,
        summary: command_result.summary.clone(),
    }
}

pub(crate) fn tool_result_summary_dto_from_protocol(
    summary: &ToolResultSummary,
) -> ToolResultSummaryDto {
    match summary {
        ToolResultSummary::Command(summary) => {
            ToolResultSummaryDto::Command(CommandToolResultSummaryDto {
                exit_code: summary.exit_code,
                timed_out: summary.timed_out,
                stdout_truncated: summary.stdout_truncated,
                stderr_truncated: summary.stderr_truncated,
                stdout_redacted: summary.stdout_redacted,
                stderr_redacted: summary.stderr_redacted,
            })
        }
        ToolResultSummary::File(summary) => ToolResultSummaryDto::File(FileToolResultSummaryDto {
            path: summary.path.clone(),
            scope: summary.scope.clone(),
            line_count: summary.line_count,
            match_count: summary.match_count,
            truncated: summary.truncated,
        }),
        ToolResultSummary::Git(summary) => ToolResultSummaryDto::Git(GitToolResultSummaryDto {
            scope: summary.scope.clone().map(git_tool_result_scope_dto),
            changed_files: summary.changed_files,
            truncated: summary.truncated,
            base_revision: summary.base_revision.clone(),
        }),
        ToolResultSummary::Web(summary) => ToolResultSummaryDto::Web(WebToolResultSummaryDto {
            target: summary.target.clone(),
            result_count: summary.result_count,
            final_url: summary.final_url.clone(),
            content_kind: summary
                .content_kind
                .clone()
                .map(web_tool_result_content_kind_dto),
            content_type: summary.content_type.clone(),
            truncated: summary.truncated,
        }),
    }
}

pub(crate) fn autonomous_skill_lifecycle_stage_dto_from_protocol(
    stage: SupervisorSkillLifecycleStage,
) -> AutonomousSkillLifecycleStageDto {
    match stage {
        SupervisorSkillLifecycleStage::Discovery => AutonomousSkillLifecycleStageDto::Discovery,
        SupervisorSkillLifecycleStage::Install => AutonomousSkillLifecycleStageDto::Install,
        SupervisorSkillLifecycleStage::Invoke => AutonomousSkillLifecycleStageDto::Invoke,
    }
}

pub(crate) fn autonomous_skill_lifecycle_result_dto_from_protocol(
    result: SupervisorSkillLifecycleResult,
) -> AutonomousSkillLifecycleResultDto {
    match result {
        SupervisorSkillLifecycleResult::Succeeded => AutonomousSkillLifecycleResultDto::Succeeded,
        SupervisorSkillLifecycleResult::Failed => AutonomousSkillLifecycleResultDto::Failed,
    }
}

pub(crate) fn autonomous_skill_lifecycle_source_dto_from_protocol(
    source: &SupervisorSkillSourceMetadata,
) -> AutonomousSkillLifecycleSourceDto {
    AutonomousSkillLifecycleSourceDto {
        repo: source.repo.clone(),
        path: source.path.clone(),
        reference: source.reference.clone(),
        tree_hash: source.tree_hash.clone(),
    }
}

pub(crate) fn autonomous_skill_cache_status_dto_from_protocol(
    status: SupervisorSkillCacheStatus,
) -> AutonomousSkillCacheStatusDto {
    match status {
        SupervisorSkillCacheStatus::Miss => AutonomousSkillCacheStatusDto::Miss,
        SupervisorSkillCacheStatus::Hit => AutonomousSkillCacheStatusDto::Hit,
        SupervisorSkillCacheStatus::Refreshed => AutonomousSkillCacheStatusDto::Refreshed,
    }
}

pub(crate) fn autonomous_skill_lifecycle_diagnostic_dto_from_protocol(
    diagnostic: &SupervisorSkillDiagnostic,
) -> AutonomousSkillLifecycleDiagnosticDto {
    AutonomousSkillLifecycleDiagnosticDto {
        code: diagnostic.code.clone(),
        message: diagnostic.message.clone(),
        retryable: diagnostic.retryable,
    }
}

fn git_tool_result_scope_dto(scope: GitToolResultScope) -> GitToolResultScopeDto {
    match scope {
        GitToolResultScope::Staged => GitToolResultScopeDto::Staged,
        GitToolResultScope::Unstaged => GitToolResultScopeDto::Unstaged,
        GitToolResultScope::Worktree => GitToolResultScopeDto::Worktree,
    }
}

fn web_tool_result_content_kind_dto(kind: WebToolResultContentKind) -> WebToolResultContentKindDto {
    match kind {
        WebToolResultContentKind::Html => WebToolResultContentKindDto::Html,
        WebToolResultContentKind::PlainText => WebToolResultContentKindDto::PlainText,
    }
}

fn autonomous_skill_lifecycle_source_dto_from_record(
    source: &AutonomousSkillLifecycleSourceRecord,
) -> AutonomousSkillLifecycleSourceDto {
    AutonomousSkillLifecycleSourceDto {
        repo: source.repo.clone(),
        path: source.path.clone(),
        reference: source.reference.clone(),
        tree_hash: source.tree_hash.clone(),
    }
}

fn autonomous_skill_lifecycle_cache_dto_from_record(
    cache: &crate::db::project_store::AutonomousSkillLifecycleCacheRecord,
) -> AutonomousSkillLifecycleCacheDto {
    AutonomousSkillLifecycleCacheDto {
        key: cache.key.clone(),
        status: cache.status.as_ref().map(autonomous_skill_cache_status_dto),
    }
}

fn autonomous_skill_lifecycle_diagnostic_dto_from_record(
    diagnostic: &AutonomousSkillLifecycleDiagnosticRecord,
) -> AutonomousSkillLifecycleDiagnosticDto {
    AutonomousSkillLifecycleDiagnosticDto {
        code: diagnostic.code.clone(),
        message: diagnostic.message.clone(),
        retryable: diagnostic.retryable,
    }
}

fn autonomous_skill_lifecycle_stage_dto(
    stage: &AutonomousSkillLifecycleStageRecord,
) -> AutonomousSkillLifecycleStageDto {
    match stage {
        AutonomousSkillLifecycleStageRecord::Discovery => {
            AutonomousSkillLifecycleStageDto::Discovery
        }
        AutonomousSkillLifecycleStageRecord::Install => AutonomousSkillLifecycleStageDto::Install,
        AutonomousSkillLifecycleStageRecord::Invoke => AutonomousSkillLifecycleStageDto::Invoke,
    }
}

fn autonomous_skill_lifecycle_result_dto(
    result: &AutonomousSkillLifecycleResultRecord,
) -> AutonomousSkillLifecycleResultDto {
    match result {
        AutonomousSkillLifecycleResultRecord::Succeeded => {
            AutonomousSkillLifecycleResultDto::Succeeded
        }
        AutonomousSkillLifecycleResultRecord::Failed => AutonomousSkillLifecycleResultDto::Failed,
    }
}

fn autonomous_skill_cache_status_dto(
    status: &AutonomousSkillCacheStatusRecord,
) -> AutonomousSkillCacheStatusDto {
    match status {
        AutonomousSkillCacheStatusRecord::Miss => AutonomousSkillCacheStatusDto::Miss,
        AutonomousSkillCacheStatusRecord::Hit => AutonomousSkillCacheStatusDto::Hit,
        AutonomousSkillCacheStatusRecord::Refreshed => AutonomousSkillCacheStatusDto::Refreshed,
    }
}

fn autonomous_tool_call_state_dto(
    state: &AutonomousToolCallStateRecord,
) -> AutonomousToolCallStateDto {
    match state {
        AutonomousToolCallStateRecord::Pending => AutonomousToolCallStateDto::Pending,
        AutonomousToolCallStateRecord::Running => AutonomousToolCallStateDto::Running,
        AutonomousToolCallStateRecord::Succeeded => AutonomousToolCallStateDto::Succeeded,
        AutonomousToolCallStateRecord::Failed => AutonomousToolCallStateDto::Failed,
    }
}

fn autonomous_verification_outcome_dto(
    outcome: &AutonomousVerificationOutcomeRecord,
) -> AutonomousVerificationOutcomeDto {
    match outcome {
        AutonomousVerificationOutcomeRecord::Passed => AutonomousVerificationOutcomeDto::Passed,
        AutonomousVerificationOutcomeRecord::Failed => AutonomousVerificationOutcomeDto::Failed,
        AutonomousVerificationOutcomeRecord::Blocked => AutonomousVerificationOutcomeDto::Blocked,
    }
}

fn autonomous_history_entry_dto_from_record(
    history: AutonomousUnitHistoryRecord,
) -> AutonomousUnitHistoryEntryDto {
    AutonomousUnitHistoryEntryDto {
        unit: autonomous_unit_dto_from_record(&history.unit),
        latest_attempt: history
            .latest_attempt
            .as_ref()
            .map(autonomous_attempt_dto_from_record),
        artifacts: history
            .artifacts
            .iter()
            .map(autonomous_artifact_dto_from_record)
            .collect(),
    }
}

pub(crate) fn generate_runtime_run_id() -> String {
    let mut bytes = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "run-{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn runtime_run_status_dto(status: RuntimeRunStatus) -> RuntimeRunStatusDto {
    match status {
        RuntimeRunStatus::Starting => RuntimeRunStatusDto::Starting,
        RuntimeRunStatus::Running => RuntimeRunStatusDto::Running,
        RuntimeRunStatus::Stale => RuntimeRunStatusDto::Stale,
        RuntimeRunStatus::Stopped => RuntimeRunStatusDto::Stopped,
        RuntimeRunStatus::Failed => RuntimeRunStatusDto::Failed,
    }
}

fn runtime_run_transport_liveness_dto(
    liveness: RuntimeRunTransportLiveness,
) -> RuntimeRunTransportLivenessDto {
    match liveness {
        RuntimeRunTransportLiveness::Unknown => RuntimeRunTransportLivenessDto::Unknown,
        RuntimeRunTransportLiveness::Reachable => RuntimeRunTransportLivenessDto::Reachable,
        RuntimeRunTransportLiveness::Unreachable => RuntimeRunTransportLivenessDto::Unreachable,
    }
}

fn runtime_run_checkpoint_kind_dto(kind: RuntimeRunCheckpointKind) -> RuntimeRunCheckpointKindDto {
    match kind {
        RuntimeRunCheckpointKind::Bootstrap => RuntimeRunCheckpointKindDto::Bootstrap,
        RuntimeRunCheckpointKind::State => RuntimeRunCheckpointKindDto::State,
        RuntimeRunCheckpointKind::Tool => RuntimeRunCheckpointKindDto::Tool,
        RuntimeRunCheckpointKind::ActionRequired => RuntimeRunCheckpointKindDto::ActionRequired,
        RuntimeRunCheckpointKind::Diagnostic => RuntimeRunCheckpointKindDto::Diagnostic,
    }
}

fn autonomous_run_status_dto(status: &AutonomousRunStatus) -> AutonomousRunStatusDto {
    match status {
        AutonomousRunStatus::Starting => AutonomousRunStatusDto::Starting,
        AutonomousRunStatus::Running => AutonomousRunStatusDto::Running,
        AutonomousRunStatus::Paused => AutonomousRunStatusDto::Paused,
        AutonomousRunStatus::Cancelling => AutonomousRunStatusDto::Cancelling,
        AutonomousRunStatus::Cancelled => AutonomousRunStatusDto::Cancelled,
        AutonomousRunStatus::Stale => AutonomousRunStatusDto::Stale,
        AutonomousRunStatus::Failed => AutonomousRunStatusDto::Failed,
        AutonomousRunStatus::Stopped => AutonomousRunStatusDto::Stopped,
        AutonomousRunStatus::Crashed => AutonomousRunStatusDto::Crashed,
        AutonomousRunStatus::Completed => AutonomousRunStatusDto::Completed,
    }
}

fn autonomous_run_recovery_state_dto(
    status: &AutonomousRunStatus,
) -> AutonomousRunRecoveryStateDto {
    match status {
        AutonomousRunStatus::Starting
        | AutonomousRunStatus::Running
        | AutonomousRunStatus::Paused => AutonomousRunRecoveryStateDto::Healthy,
        AutonomousRunStatus::Cancelling | AutonomousRunStatus::Stale => {
            AutonomousRunRecoveryStateDto::RecoveryRequired
        }
        AutonomousRunStatus::Cancelled
        | AutonomousRunStatus::Stopped
        | AutonomousRunStatus::Completed => AutonomousRunRecoveryStateDto::Terminal,
        AutonomousRunStatus::Failed | AutonomousRunStatus::Crashed => {
            AutonomousRunRecoveryStateDto::Failed
        }
    }
}

fn autonomous_unit_kind_dto(kind: &AutonomousUnitKind) -> AutonomousUnitKindDto {
    match kind {
        AutonomousUnitKind::Researcher => AutonomousUnitKindDto::Researcher,
        AutonomousUnitKind::Planner => AutonomousUnitKindDto::Planner,
        AutonomousUnitKind::Executor => AutonomousUnitKindDto::Executor,
        AutonomousUnitKind::Verifier => AutonomousUnitKindDto::Verifier,
    }
}

fn autonomous_unit_status_dto(status: &AutonomousUnitStatus) -> AutonomousUnitStatusDto {
    match status {
        AutonomousUnitStatus::Pending => AutonomousUnitStatusDto::Pending,
        AutonomousUnitStatus::Active => AutonomousUnitStatusDto::Active,
        AutonomousUnitStatus::Blocked => AutonomousUnitStatusDto::Blocked,
        AutonomousUnitStatus::Paused => AutonomousUnitStatusDto::Paused,
        AutonomousUnitStatus::Completed => AutonomousUnitStatusDto::Completed,
        AutonomousUnitStatus::Cancelled => AutonomousUnitStatusDto::Cancelled,
        AutonomousUnitStatus::Failed => AutonomousUnitStatusDto::Failed,
    }
}

fn autonomous_artifact_status_dto(
    status: &AutonomousUnitArtifactStatus,
) -> AutonomousUnitArtifactStatusDto {
    match status {
        AutonomousUnitArtifactStatus::Pending => AutonomousUnitArtifactStatusDto::Pending,
        AutonomousUnitArtifactStatus::Recorded => AutonomousUnitArtifactStatusDto::Recorded,
        AutonomousUnitArtifactStatus::Rejected => AutonomousUnitArtifactStatusDto::Rejected,
        AutonomousUnitArtifactStatus::Redacted => AutonomousUnitArtifactStatusDto::Redacted,
    }
}

fn runtime_reason_dto(reason: &RuntimeRunDiagnosticRecord) -> AutonomousLifecycleReasonDto {
    AutonomousLifecycleReasonDto {
        code: reason.code.clone(),
        message: reason.message.clone(),
    }
}

fn runtime_run_diagnostic_dto(reason: &RuntimeRunDiagnosticRecord) -> RuntimeRunDiagnosticDto {
    RuntimeRunDiagnosticDto {
        code: reason.code.clone(),
        message: reason.message.clone(),
    }
}

pub(crate) fn command_error_from_auth(error: AuthFlowError) -> CommandError {
    let class = if error.retryable {
        CommandErrorClass::Retryable
    } else {
        CommandErrorClass::UserFixable
    };

    CommandError::new(error.code, class, error.message, error.retryable)
}

pub(crate) fn runtime_diagnostic_from_auth(error: AuthDiagnostic) -> RuntimeDiagnosticDto {
    RuntimeDiagnosticDto {
        code: error.code,
        message: error.message,
        retryable: error.retryable,
    }
}

fn runtime_diagnostic_from_record(error: RuntimeSessionDiagnosticRecord) -> RuntimeDiagnosticDto {
    RuntimeDiagnosticDto {
        code: error.code,
        message: error.message,
        retryable: error.retryable,
    }
}
