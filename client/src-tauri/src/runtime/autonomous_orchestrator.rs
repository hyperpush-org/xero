use std::path::Path;

use rand::RngCore;

use super::autonomous_workflow_progression::persist_autonomous_workflow_progression;

use crate::{
    auth::now_timestamp,
    commands::CommandError,
    db::project_store::{
        self, AutonomousArtifactCommandResultRecord, AutonomousArtifactPayloadRecord,
        AutonomousPolicyDeniedPayloadRecord, AutonomousRunRecord, AutonomousRunSnapshotRecord,
        AutonomousRunStatus, AutonomousRunUpsertRecord, AutonomousToolCallStateRecord,
        AutonomousToolResultPayloadRecord, AutonomousUnitArtifactRecord,
        AutonomousUnitArtifactStatus, AutonomousUnitAttemptRecord, AutonomousUnitKind,
        AutonomousUnitRecord, AutonomousUnitStatus, AutonomousVerificationEvidencePayloadRecord,
        AutonomousVerificationOutcomeRecord, RuntimeRunDiagnosticRecord, RuntimeRunSnapshotRecord,
        RuntimeRunStatus,
    },
    runtime::protocol::{
        CommandToolResultSummary, SupervisorLiveEventPayload, SupervisorToolCallState,
        ToolResultSummary,
    },
};

const AUTONOMOUS_DUPLICATE_START_REASON: &str =
    "Cadence reused the already-active autonomous run for this project instead of launching a duplicate supervisor.";
const AUTONOMOUS_CANCEL_REASON_CODE: &str = "autonomous_run_cancelled";
const AUTONOMOUS_CANCEL_REASON_MESSAGE: &str =
    "Operator cancelled the autonomous run from the desktop shell.";
const AUTONOMOUS_BOUNDARY_PAUSE_CODE: &str = "autonomous_operator_action_required";
const AUTONOMOUS_BOUNDARY_RESUME_EVIDENCE_KIND: &str = "operator_resume";
const AUTONOMOUS_BOUNDARY_RESUME_LABEL: &str = "Operator resumed the blocked boundary.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomousRuntimeReconcileIntent {
    Observe,
    DuplicateStart,
    CancelRequested,
}

pub fn reconcile_runtime_snapshot(
    existing: Option<&AutonomousRunSnapshotRecord>,
    runtime_snapshot: &RuntimeRunSnapshotRecord,
    intent: AutonomousRuntimeReconcileIntent,
) -> AutonomousRunUpsertRecord {
    let is_same_run =
        existing.is_some_and(|existing| existing.run.run_id == runtime_snapshot.run.run_id);
    let existing_run = is_same_run.then(|| existing.expect("checked same-run autonomous snapshot"));
    let existing_unit = existing_run.and_then(|snapshot| snapshot.unit.as_ref());
    let existing_attempt = existing_run.and_then(|snapshot| snapshot.attempt.as_ref());

    let duplicate_start_detected =
        matches!(intent, AutonomousRuntimeReconcileIntent::DuplicateStart)
            || existing_run
                .map(|snapshot| snapshot.run.duplicate_start_detected)
                .unwrap_or(false);
    let duplicate_start_run_id =
        duplicate_start_detected.then(|| runtime_snapshot.run.run_id.clone());
    let duplicate_start_reason =
        duplicate_start_detected.then_some(AUTONOMOUS_DUPLICATE_START_REASON.to_string());

    let base_updated_at = if matches!(intent, AutonomousRuntimeReconcileIntent::DuplicateStart) {
        now_timestamp()
    } else {
        runtime_snapshot.run.updated_at.clone()
    };

    let last_error = runtime_snapshot.run.last_error.clone();
    let existing_blocked_boundary = existing_attempt
        .and_then(|attempt| attempt.boundary_id.clone())
        .filter(|boundary_id| !boundary_id.trim().is_empty())
        .zip(existing_run)
        .filter(|(_, snapshot)| {
            matches!(
                snapshot.run.status,
                AutonomousRunStatus::Paused
                    | AutonomousRunStatus::Running
                    | AutonomousRunStatus::Stale
            ) && matches!(
                snapshot.attempt.as_ref().map(|attempt| &attempt.status),
                Some(AutonomousUnitStatus::Blocked | AutonomousUnitStatus::Paused)
            )
        })
        .map(|(boundary_id, _)| boundary_id);

    let (status, cancelled_at, cancel_reason) = match runtime_snapshot.run.status {
        RuntimeRunStatus::Stopped
            if matches!(intent, AutonomousRuntimeReconcileIntent::CancelRequested) =>
        {
            (
                AutonomousRunStatus::Cancelled,
                runtime_snapshot
                    .run
                    .stopped_at
                    .clone()
                    .or_else(|| Some(now_timestamp())),
                Some(RuntimeRunDiagnosticRecord {
                    code: AUTONOMOUS_CANCEL_REASON_CODE.into(),
                    message: AUTONOMOUS_CANCEL_REASON_MESSAGE.into(),
                }),
            )
        }
        RuntimeRunStatus::Stopped => (
            existing_run
                .map(|snapshot| snapshot.run.status.clone())
                .filter(|status| {
                    matches!(
                        status,
                        AutonomousRunStatus::Cancelled | AutonomousRunStatus::Completed
                    )
                })
                .unwrap_or(AutonomousRunStatus::Stopped),
            existing_run.and_then(|snapshot| snapshot.run.cancelled_at.clone()),
            existing_run.and_then(|snapshot| snapshot.run.cancel_reason.clone()),
        ),
        RuntimeRunStatus::Starting | RuntimeRunStatus::Running
            if existing_blocked_boundary.is_some() =>
        {
            (
                AutonomousRunStatus::Paused,
                existing_run.and_then(|snapshot| snapshot.run.cancelled_at.clone()),
                existing_run.and_then(|snapshot| snapshot.run.cancel_reason.clone()),
            )
        }
        RuntimeRunStatus::Starting => (AutonomousRunStatus::Starting, None, None),
        RuntimeRunStatus::Running => (AutonomousRunStatus::Running, None, None),
        RuntimeRunStatus::Stale => (AutonomousRunStatus::Stale, None, None),
        RuntimeRunStatus::Failed => (AutonomousRunStatus::Failed, None, None),
    };

    let (crashed_at, crash_reason) = match runtime_snapshot.run.status {
        RuntimeRunStatus::Stale | RuntimeRunStatus::Failed => (
            existing_run
                .and_then(|snapshot| snapshot.run.crashed_at.clone())
                .or_else(|| Some(runtime_snapshot.run.updated_at.clone())),
            last_error.clone(),
        ),
        _ => (None, None),
    };

    let completed_at = existing_run
        .and_then(|snapshot| snapshot.run.completed_at.clone())
        .filter(|_| matches!(status, AutonomousRunStatus::Completed));
    let paused_at = if matches!(status, AutonomousRunStatus::Paused) {
        existing_run
            .and_then(|snapshot| snapshot.run.paused_at.clone())
            .or_else(|| Some(base_updated_at.clone()))
    } else {
        None
    };
    let pause_reason = if matches!(status, AutonomousRunStatus::Paused) {
        existing_run
            .and_then(|snapshot| snapshot.run.pause_reason.clone())
            .or_else(|| {
                Some(RuntimeRunDiagnosticRecord {
                    code: AUTONOMOUS_BOUNDARY_PAUSE_CODE.into(),
                    message: "Cadence paused the active autonomous attempt until the operator resolves the pending boundary.".into(),
                })
            })
    } else {
        None
    };

    let sequence = existing_unit.map(|unit| unit.sequence).unwrap_or(1);
    let unit_id = existing_unit
        .map(|unit| unit.unit_id.clone())
        .unwrap_or_else(|| format!("{}:unit:{}", runtime_snapshot.run.run_id, sequence));
    let attempt_number = existing_attempt
        .map(|attempt| attempt.attempt_number)
        .unwrap_or(1);
    let attempt_id = existing_attempt
        .map(|attempt| attempt.attempt_id.clone())
        .unwrap_or_else(|| format!("{unit_id}:attempt:{attempt_number}"));
    let child_session_id = existing_attempt
        .map(|attempt| attempt.child_session_id.clone())
        .unwrap_or_else(generate_autonomous_child_session_id);
    let unit_summary = existing_unit
        .map(|unit| unit.summary.clone())
        .or_else(|| {
            runtime_snapshot
                .checkpoints
                .last()
                .map(|checkpoint| checkpoint.summary.clone())
        })
        .unwrap_or_else(|| "Researcher child session launched.".to_string());

    let blocked_by_boundary = existing_blocked_boundary.is_some();
    let unit_status = if blocked_by_boundary
        && matches!(
            runtime_snapshot.run.status,
            RuntimeRunStatus::Starting | RuntimeRunStatus::Running | RuntimeRunStatus::Stale
        ) {
        AutonomousUnitStatus::Blocked
    } else {
        autonomous_unit_status_for_run(&status)
    };
    let finished_at = match unit_status {
        AutonomousUnitStatus::Completed
        | AutonomousUnitStatus::Cancelled
        | AutonomousUnitStatus::Failed => Some(base_updated_at.clone()),
        _ => None,
    };

    let boundary_id = existing_attempt
        .and_then(|attempt| attempt.boundary_id.clone())
        .filter(|_| blocked_by_boundary);
    let unit = AutonomousUnitRecord {
        project_id: runtime_snapshot.run.project_id.clone(),
        run_id: runtime_snapshot.run.run_id.clone(),
        unit_id: unit_id.clone(),
        sequence,
        kind: existing_unit
            .map(|unit| unit.kind.clone())
            .unwrap_or(AutonomousUnitKind::Researcher),
        status: unit_status.clone(),
        summary: unit_summary,
        boundary_id: boundary_id.clone(),
        workflow_linkage: existing_unit.and_then(|unit| unit.workflow_linkage.clone()),
        started_at: existing_unit
            .map(|unit| unit.started_at.clone())
            .unwrap_or_else(|| runtime_snapshot.run.started_at.clone()),
        finished_at: finished_at.clone(),
        updated_at: base_updated_at.clone(),
        last_error: last_error.clone(),
    };
    let attempt = AutonomousUnitAttemptRecord {
        project_id: runtime_snapshot.run.project_id.clone(),
        run_id: runtime_snapshot.run.run_id.clone(),
        unit_id: unit_id.clone(),
        attempt_id: attempt_id.clone(),
        attempt_number,
        child_session_id,
        status: unit_status,
        boundary_id,
        workflow_linkage: existing_attempt.and_then(|attempt| attempt.workflow_linkage.clone()),
        started_at: existing_attempt
            .map(|attempt| attempt.started_at.clone())
            .unwrap_or_else(|| runtime_snapshot.run.started_at.clone()),
        finished_at,
        updated_at: base_updated_at.clone(),
        last_error: last_error.clone(),
    };

    AutonomousRunUpsertRecord {
        run: AutonomousRunRecord {
            project_id: runtime_snapshot.run.project_id.clone(),
            run_id: runtime_snapshot.run.run_id.clone(),
            runtime_kind: runtime_snapshot.run.runtime_kind.clone(),
            supervisor_kind: runtime_snapshot.run.supervisor_kind.clone(),
            status,
            active_unit_sequence: Some(sequence),
            duplicate_start_detected,
            duplicate_start_run_id,
            duplicate_start_reason,
            started_at: runtime_snapshot.run.started_at.clone(),
            last_heartbeat_at: runtime_snapshot.run.last_heartbeat_at.clone(),
            last_checkpoint_at: runtime_snapshot.last_checkpoint_at.clone(),
            paused_at,
            cancelled_at,
            completed_at,
            crashed_at,
            stopped_at: runtime_snapshot.run.stopped_at.clone(),
            pause_reason,
            cancel_reason,
            crash_reason,
            last_error,
            updated_at: base_updated_at,
        },
        unit: Some(unit),
        attempt: Some(attempt),
        artifacts: current_attempt_artifacts(existing_run),
    }
}

pub fn persist_supervisor_event(
    repo_root: &Path,
    project_id: &str,
    event: &SupervisorLiveEventPayload,
) -> Result<Option<AutonomousRunSnapshotRecord>, CommandError> {
    let runtime_snapshot = match project_store::load_runtime_run(repo_root, project_id)? {
        Some(snapshot) => snapshot,
        None => return Ok(None),
    };
    let existing = project_store::load_autonomous_run(repo_root, project_id)?;
    if let Some(snapshot) = existing.as_ref() {
        if snapshot.run.run_id != runtime_snapshot.run.run_id {
            return Err(CommandError::retryable(
                "autonomous_live_event_run_mismatch",
                format!(
                    "Cadence refused to persist live supervisor event state because the durable autonomous run `{}` does not match active runtime run `{}` for project `{project_id}`.",
                    snapshot.run.run_id, runtime_snapshot.run.run_id,
                ),
            ));
        }
    }

    let mut payload = reconcile_runtime_snapshot(
        existing.as_ref(),
        &runtime_snapshot,
        AutonomousRuntimeReconcileIntent::Observe,
    );
    match event {
        SupervisorLiveEventPayload::Tool {
            tool_call_id,
            tool_name,
            tool_state,
            detail,
            tool_summary,
        } => {
            let Some(attempt) = payload.attempt.as_ref() else {
                return Ok(None);
            };
            let state_label = supervisor_tool_state_label(tool_state);
            let artifact_id = format!(
                "{}:tool:{}:{}",
                attempt.attempt_id, tool_call_id, state_label
            );
            let timestamp = existing_artifact_timestamp(existing.as_ref(), &artifact_id)
                .unwrap_or_else(now_timestamp);
            let command_result = tool_summary.as_ref().and_then(|summary| {
                command_result_record_for_tool_summary(summary, detail.as_deref())
            });
            upsert_artifact(
                &mut payload.artifacts,
                AutonomousUnitArtifactRecord {
                    project_id: attempt.project_id.clone(),
                    run_id: attempt.run_id.clone(),
                    unit_id: attempt.unit_id.clone(),
                    attempt_id: attempt.attempt_id.clone(),
                    artifact_id: artifact_id.clone(),
                    artifact_kind: "tool_result".into(),
                    status: AutonomousUnitArtifactStatus::Recorded,
                    summary: detail.clone().unwrap_or_else(|| {
                        format!(
                            "Tool `{tool_name}` {state_label} for the active autonomous attempt."
                        )
                    }),
                    content_hash: None,
                    payload: Some(AutonomousArtifactPayloadRecord::ToolResult(
                        AutonomousToolResultPayloadRecord {
                            project_id: attempt.project_id.clone(),
                            run_id: attempt.run_id.clone(),
                            unit_id: attempt.unit_id.clone(),
                            attempt_id: attempt.attempt_id.clone(),
                            artifact_id,
                            tool_call_id: tool_call_id.clone(),
                            tool_name: tool_name.clone(),
                            tool_state: supervisor_tool_state_record(tool_state),
                            command_result,
                            tool_summary: tool_summary.clone(),
                            action_id: None,
                            boundary_id: None,
                        },
                    )),
                    created_at: timestamp.clone(),
                    updated_at: now_timestamp(),
                },
            );
        }
        SupervisorLiveEventPayload::ActionRequired {
            action_id,
            boundary_id,
            action_type,
            title,
            detail,
        } => {
            let timestamp = now_timestamp();
            payload.run.status = AutonomousRunStatus::Paused;
            payload.run.paused_at = Some(timestamp.clone());
            payload.run.pause_reason = Some(RuntimeRunDiagnosticRecord {
                code: AUTONOMOUS_BOUNDARY_PAUSE_CODE.into(),
                message: detail.clone(),
            });
            payload.run.updated_at = timestamp.clone();

            if let Some(unit) = payload.unit.as_mut() {
                unit.status = AutonomousUnitStatus::Blocked;
                unit.summary = format!("Blocked on operator boundary `{title}`.");
                unit.boundary_id = Some(boundary_id.clone());
                unit.finished_at = None;
                unit.updated_at = timestamp.clone();
            }
            if let Some(attempt) = payload.attempt.as_mut() {
                attempt.status = AutonomousUnitStatus::Blocked;
                attempt.boundary_id = Some(boundary_id.clone());
                attempt.finished_at = None;
                attempt.updated_at = timestamp.clone();

                let artifact_id =
                    format!("{}:boundary:{}:blocked", attempt.attempt_id, boundary_id);
                let created_at = existing_artifact_timestamp(existing.as_ref(), &artifact_id)
                    .unwrap_or_else(|| timestamp.clone());
                upsert_artifact(
                    &mut payload.artifacts,
                    AutonomousUnitArtifactRecord {
                        project_id: attempt.project_id.clone(),
                        run_id: attempt.run_id.clone(),
                        unit_id: attempt.unit_id.clone(),
                        attempt_id: attempt.attempt_id.clone(),
                        artifact_id: artifact_id.clone(),
                        artifact_kind: "verification_evidence".into(),
                        status: AutonomousUnitArtifactStatus::Recorded,
                        summary: format!(
                            "Autonomous attempt blocked on `{title}` and is waiting for operator action."
                        ),
                        content_hash: None,
                        payload: Some(AutonomousArtifactPayloadRecord::VerificationEvidence(
                            AutonomousVerificationEvidencePayloadRecord {
                                project_id: attempt.project_id.clone(),
                                run_id: attempt.run_id.clone(),
                                unit_id: attempt.unit_id.clone(),
                                attempt_id: attempt.attempt_id.clone(),
                                artifact_id,
                                evidence_kind: action_type.clone(),
                                label: title.clone(),
                                outcome: AutonomousVerificationOutcomeRecord::Blocked,
                                command_result: None,
                                action_id: Some(action_id.clone()),
                                boundary_id: Some(boundary_id.clone()),
                            },
                        )),
                        created_at,
                        updated_at: timestamp,
                    },
                );
            }
        }
        SupervisorLiveEventPayload::Activity {
            code,
            title,
            detail,
        } if code.contains("policy_denied") => {
            if let Some(attempt) = payload.attempt.as_ref() {
                let artifact_id = format!(
                    "{}:policy:{}",
                    attempt.attempt_id,
                    sanitize_artifact_fragment(code)
                );
                let timestamp = existing_artifact_timestamp(existing.as_ref(), &artifact_id)
                    .unwrap_or_else(now_timestamp);
                upsert_artifact(
                    &mut payload.artifacts,
                    AutonomousUnitArtifactRecord {
                        project_id: attempt.project_id.clone(),
                        run_id: attempt.run_id.clone(),
                        unit_id: attempt.unit_id.clone(),
                        attempt_id: attempt.attempt_id.clone(),
                        artifact_id: artifact_id.clone(),
                        artifact_kind: "policy_denied".into(),
                        status: AutonomousUnitArtifactStatus::Recorded,
                        summary: detail.clone().unwrap_or_else(|| title.clone()),
                        content_hash: None,
                        payload: Some(AutonomousArtifactPayloadRecord::PolicyDenied(
                            AutonomousPolicyDeniedPayloadRecord {
                                project_id: attempt.project_id.clone(),
                                run_id: attempt.run_id.clone(),
                                unit_id: attempt.unit_id.clone(),
                                attempt_id: attempt.attempt_id.clone(),
                                artifact_id,
                                diagnostic_code: code.clone(),
                                message: detail.clone().unwrap_or_else(|| title.clone()),
                                tool_name: None,
                                action_id: None,
                                boundary_id: None,
                            },
                        )),
                        created_at: timestamp.clone(),
                        updated_at: now_timestamp(),
                    },
                );
            }
        }
        _ => return Ok(None),
    }

    persist_progressed_autonomous_run(repo_root, project_id, existing.as_ref(), payload).map(Some)
}

#[derive(Debug, Clone)]
struct OperatorResumeGuard {
    runtime_snapshot: RuntimeRunSnapshotRecord,
    existing: AutonomousRunSnapshotRecord,
    blocked_artifact: AutonomousUnitArtifactRecord,
}

pub fn validate_operator_resume_target(
    repo_root: &Path,
    project_id: &str,
    action_id: &str,
    boundary_id: &str,
) -> Result<(), CommandError> {
    load_operator_resume_guard(repo_root, project_id, action_id, boundary_id).map(|_| ())
}

pub fn persist_operator_resume(
    repo_root: &Path,
    project_id: &str,
    action_id: &str,
    boundary_id: &str,
) -> Result<Option<AutonomousRunSnapshotRecord>, CommandError> {
    let Some(OperatorResumeGuard {
        runtime_snapshot,
        existing,
        blocked_artifact,
    }) = load_operator_resume_guard(repo_root, project_id, action_id, boundary_id)?
    else {
        return Ok(None);
    };

    let mut payload = reconcile_runtime_snapshot(
        Some(&existing),
        &runtime_snapshot,
        AutonomousRuntimeReconcileIntent::Observe,
    );
    let timestamp = now_timestamp();
    payload.run.status = match runtime_snapshot.run.status {
        RuntimeRunStatus::Starting | RuntimeRunStatus::Running => AutonomousRunStatus::Running,
        RuntimeRunStatus::Stale => AutonomousRunStatus::Stale,
        RuntimeRunStatus::Failed => AutonomousRunStatus::Failed,
        RuntimeRunStatus::Stopped => AutonomousRunStatus::Stopped,
    };
    payload.run.paused_at = None;
    payload.run.pause_reason = None;
    payload.run.updated_at = timestamp.clone();

    if let Some(unit) = payload.unit.as_mut() {
        unit.status = AutonomousUnitStatus::Active;
        unit.boundary_id = None;
        unit.finished_at = None;
        unit.summary = "Autonomous attempt resumed after operator approval.".into();
        unit.updated_at = timestamp.clone();
        unit.last_error = None;
    }
    if let Some(attempt) = payload.attempt.as_mut() {
        attempt.status = AutonomousUnitStatus::Active;
        attempt.boundary_id = None;
        attempt.finished_at = None;
        attempt.updated_at = timestamp.clone();
        attempt.last_error = None;

        let artifact_id = format!("{}:boundary:{}:resumed", attempt.attempt_id, boundary_id);
        let created_at = existing_artifact_timestamp(Some(&existing), &artifact_id)
            .unwrap_or_else(|| timestamp.clone());
        let label = match blocked_artifact.payload.as_ref() {
            Some(AutonomousArtifactPayloadRecord::VerificationEvidence(evidence)) => {
                evidence.label.clone()
            }
            _ => AUTONOMOUS_BOUNDARY_RESUME_LABEL.into(),
        };
        upsert_artifact(
            &mut payload.artifacts,
            AutonomousUnitArtifactRecord {
                project_id: attempt.project_id.clone(),
                run_id: attempt.run_id.clone(),
                unit_id: attempt.unit_id.clone(),
                attempt_id: attempt.attempt_id.clone(),
                artifact_id: artifact_id.clone(),
                artifact_kind: "verification_evidence".into(),
                status: AutonomousUnitArtifactStatus::Recorded,
                summary: format!(
                    "Autonomous attempt resumed boundary `{label}` after operator approval."
                ),
                content_hash: None,
                payload: Some(AutonomousArtifactPayloadRecord::VerificationEvidence(
                    AutonomousVerificationEvidencePayloadRecord {
                        project_id: attempt.project_id.clone(),
                        run_id: attempt.run_id.clone(),
                        unit_id: attempt.unit_id.clone(),
                        attempt_id: attempt.attempt_id.clone(),
                        artifact_id,
                        evidence_kind: AUTONOMOUS_BOUNDARY_RESUME_EVIDENCE_KIND.into(),
                        label,
                        outcome: AutonomousVerificationOutcomeRecord::Passed,
                        command_result: None,
                        action_id: Some(action_id.to_string()),
                        boundary_id: Some(boundary_id.to_string()),
                    },
                )),
                created_at,
                updated_at: timestamp,
            },
        );
    }

    persist_progressed_autonomous_run(repo_root, project_id, Some(&existing), payload).map(Some)
}

fn load_operator_resume_guard(
    repo_root: &Path,
    project_id: &str,
    action_id: &str,
    boundary_id: &str,
) -> Result<Option<OperatorResumeGuard>, CommandError> {
    let runtime_snapshot = project_store::load_runtime_run(repo_root, project_id)?;
    let existing = project_store::load_autonomous_run(repo_root, project_id)?;

    let (runtime_snapshot, existing) = match (runtime_snapshot, existing) {
        (Some(runtime_snapshot), Some(existing)) => (runtime_snapshot, existing),
        (None, Some(_)) => {
            return Err(CommandError::retryable(
                "autonomous_resume_run_mismatch",
                format!(
                    "Cadence refused to resume autonomous boundary `{boundary_id}` because the selected project no longer has a durable runtime run for action `{action_id}`."
                ),
            ))
        }
        _ => return Ok(None),
    };

    if existing.run.run_id != runtime_snapshot.run.run_id {
        return Err(CommandError::retryable(
            "autonomous_resume_run_mismatch",
            format!(
                "Cadence refused to resume autonomous boundary `{boundary_id}` because active runtime run `{}` does not match durable autonomous run `{}`.",
                runtime_snapshot.run.run_id, existing.run.run_id,
            ),
        ));
    }

    let resumed_artifact_exists = existing
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .any(|artifact| {
            matches!(
                artifact.payload.as_ref(),
                Some(AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                    if payload.action_id.as_deref() == Some(action_id)
                        && payload.boundary_id.as_deref() == Some(boundary_id)
                        && payload.outcome == AutonomousVerificationOutcomeRecord::Passed
                        && payload.evidence_kind == AUTONOMOUS_BOUNDARY_RESUME_EVIDENCE_KIND
            )
        });
    if resumed_artifact_exists {
        return Err(CommandError::user_fixable(
            "autonomous_resume_already_completed",
            format!(
                "Cadence refused to resume autonomous boundary `{boundary_id}` for action `{action_id}` because the durable evidence trail already records that this blocked boundary was resumed."
            ),
        ));
    }

    let Some(existing_attempt) = existing.attempt.as_ref() else {
        return Err(CommandError::retryable(
            "autonomous_resume_identity_invalid",
            format!(
                "Cadence cannot resume autonomous boundary `{boundary_id}` because the durable attempt ledger is missing the active attempt row."
            ),
        ));
    };
    if existing_attempt.boundary_id.as_deref() != Some(boundary_id) {
        return Err(CommandError::user_fixable(
            "autonomous_resume_boundary_mismatch",
            format!(
                "Cadence refused to resume boundary `{boundary_id}` because the active autonomous attempt is no longer blocked on that exact boundary."
            ),
        ));
    }

    let blocked_artifact = existing
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .find(|artifact| {
            matches!(
                artifact.payload.as_ref(),
                Some(AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                    if payload.action_id.as_deref() == Some(action_id)
                        && payload.boundary_id.as_deref() == Some(boundary_id)
                        && payload.outcome == AutonomousVerificationOutcomeRecord::Blocked
            )
        })
        .cloned()
        .ok_or_else(|| {
            CommandError::retryable(
                "autonomous_resume_identity_invalid",
                format!(
                    "Cadence cannot resume autonomous boundary `{boundary_id}` for action `{action_id}` because the durable evidence trail does not contain the matching blocked boundary record."
                ),
            )
        })?;

    Ok(Some(OperatorResumeGuard {
        runtime_snapshot,
        existing,
        blocked_artifact,
    }))
}

fn persist_progressed_autonomous_run(
    repo_root: &Path,
    project_id: &str,
    existing: Option<&AutonomousRunSnapshotRecord>,
    payload: AutonomousRunUpsertRecord,
) -> Result<AutonomousRunSnapshotRecord, CommandError> {
    persist_autonomous_workflow_progression(repo_root, project_id, existing, payload)
}

fn upsert_artifact(
    artifacts: &mut Vec<AutonomousUnitArtifactRecord>,
    artifact: AutonomousUnitArtifactRecord,
) {
    if let Some(index) = artifacts
        .iter()
        .position(|existing| existing.artifact_id == artifact.artifact_id)
    {
        artifacts[index] = artifact;
    } else {
        artifacts.push(artifact);
    }
}

fn current_attempt_artifacts(
    existing: Option<&AutonomousRunSnapshotRecord>,
) -> Vec<AutonomousUnitArtifactRecord> {
    let Some(existing) = existing else {
        return Vec::new();
    };
    let Some(attempt_id) = existing
        .attempt
        .as_ref()
        .map(|attempt| attempt.attempt_id.as_str())
    else {
        return Vec::new();
    };

    existing
        .history
        .iter()
        .find(|entry| {
            entry
                .latest_attempt
                .as_ref()
                .is_some_and(|attempt| attempt.attempt_id == attempt_id)
        })
        .map(|entry| entry.artifacts.clone())
        .unwrap_or_default()
}

fn existing_artifact_timestamp(
    existing: Option<&AutonomousRunSnapshotRecord>,
    artifact_id: &str,
) -> Option<String> {
    existing.and_then(|snapshot| {
        snapshot
            .history
            .iter()
            .flat_map(|entry| entry.artifacts.iter())
            .find(|artifact| artifact.artifact_id == artifact_id)
            .map(|artifact| artifact.created_at.clone())
    })
}

fn command_result_record_for_tool_summary(
    summary: &ToolResultSummary,
    detail: Option<&str>,
) -> Option<AutonomousArtifactCommandResultRecord> {
    match summary {
        ToolResultSummary::Command(CommandToolResultSummary {
            exit_code,
            timed_out,
            ..
        }) => Some(AutonomousArtifactCommandResultRecord {
            exit_code: *exit_code,
            timed_out: *timed_out,
            summary: detail
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| match (timed_out, exit_code) {
                    (true, Some(code)) => {
                        format!("Command timed out and exited with code {code}.")
                    }
                    (true, None) => "Command timed out before reporting an exit code.".into(),
                    (false, Some(0)) => "Command exited successfully.".into(),
                    (false, Some(code)) => format!("Command exited with code {code}."),
                    (false, None) => "Command terminated without an exit code.".into(),
                }),
        }),
        _ => None,
    }
}

fn supervisor_tool_state_record(state: &SupervisorToolCallState) -> AutonomousToolCallStateRecord {
    match state {
        SupervisorToolCallState::Pending => AutonomousToolCallStateRecord::Pending,
        SupervisorToolCallState::Running => AutonomousToolCallStateRecord::Running,
        SupervisorToolCallState::Succeeded => AutonomousToolCallStateRecord::Succeeded,
        SupervisorToolCallState::Failed => AutonomousToolCallStateRecord::Failed,
    }
}

fn supervisor_tool_state_label(state: &SupervisorToolCallState) -> &'static str {
    match state {
        SupervisorToolCallState::Pending => "pending",
        SupervisorToolCallState::Running => "running",
        SupervisorToolCallState::Succeeded => "succeeded",
        SupervisorToolCallState::Failed => "failed",
    }
}

fn autonomous_unit_status_for_run(status: &AutonomousRunStatus) -> AutonomousUnitStatus {
    match status {
        AutonomousRunStatus::Starting
        | AutonomousRunStatus::Running
        | AutonomousRunStatus::Stale
        | AutonomousRunStatus::Cancelling => AutonomousUnitStatus::Active,
        AutonomousRunStatus::Paused => AutonomousUnitStatus::Paused,
        AutonomousRunStatus::Cancelled => AutonomousUnitStatus::Cancelled,
        AutonomousRunStatus::Stopped | AutonomousRunStatus::Completed => {
            AutonomousUnitStatus::Completed
        }
        AutonomousRunStatus::Failed | AutonomousRunStatus::Crashed => AutonomousUnitStatus::Failed,
    }
}

fn generate_autonomous_child_session_id() -> String {
    let mut bytes = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "child-{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn sanitize_artifact_fragment(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| match character {
            ':' | '/' | '\\' | ' ' => '-',
            character
                if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') =>
            {
                character
            }
            _ => '-',
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        "event".into()
    } else {
        trimmed.into()
    }
}
