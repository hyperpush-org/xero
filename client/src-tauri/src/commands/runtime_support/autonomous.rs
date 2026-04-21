use std::path::Path;

use crate::{
    commands::{
        AutonomousArtifactPayloadDto, AutonomousCommandResultDto, AutonomousPolicyDeniedPayloadDto,
        AutonomousRunDto, AutonomousRunRecoveryStateDto, AutonomousRunStateDto,
        AutonomousRunStatusDto, AutonomousSkillLifecyclePayloadDto, AutonomousToolCallStateDto,
        AutonomousToolResultPayloadDto, AutonomousUnitArtifactDto, AutonomousUnitArtifactStatusDto,
        AutonomousUnitAttemptDto, AutonomousUnitDto, AutonomousUnitHistoryEntryDto,
        AutonomousUnitKindDto, AutonomousUnitStatusDto, AutonomousVerificationEvidencePayloadDto,
        AutonomousVerificationOutcomeDto, AutonomousWorkflowLinkageDto, CommandError,
        CommandErrorClass, CommandResult,
    },
    db::project_store::{
        self, AutonomousArtifactCommandResultRecord, AutonomousArtifactPayloadRecord,
        AutonomousRunSnapshotRecord, AutonomousRunStatus, AutonomousRunUpsertRecord,
        AutonomousToolCallStateRecord, AutonomousUnitArtifactRecord, AutonomousUnitArtifactStatus,
        AutonomousUnitAttemptRecord, AutonomousUnitHistoryRecord, AutonomousUnitKind,
        AutonomousUnitRecord, AutonomousUnitStatus, AutonomousVerificationOutcomeRecord,
        RuntimeRunSnapshotRecord,
    },
    runtime::{
        autonomous_orchestrator::{reconcile_runtime_snapshot, AutonomousRuntimeReconcileIntent},
        autonomous_workflow_progression::persist_autonomous_workflow_progression,
    },
};

use super::{
    protocol_dto::{
        autonomous_skill_lifecycle_cache_dto_from_record,
        autonomous_skill_lifecycle_diagnostic_dto_from_record,
        autonomous_skill_lifecycle_result_dto, autonomous_skill_lifecycle_source_dto_from_record,
        autonomous_skill_lifecycle_stage_dto, tool_result_summary_dto_from_protocol,
    },
    run::{provider_id_from_runtime_kind, runtime_reason_dto, runtime_run_diagnostic_dto},
};

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
