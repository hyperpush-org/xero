use std::path::Path;

use crate::{
    auth::now_timestamp,
    commands::CommandError,
    db::project_store::{
        self, AutonomousArtifactPayloadRecord, AutonomousRunSnapshotRecord, AutonomousRunStatus,
        AutonomousUnitArtifactRecord, AutonomousUnitArtifactStatus, AutonomousUnitStatus,
        AutonomousVerificationEvidencePayloadRecord, AutonomousVerificationOutcomeRecord,
        RuntimeRunSnapshotRecord, RuntimeRunStatus,
    },
};

use super::{
    existing_artifact_timestamp, persist_progressed_autonomous_run, upsert_artifact,
    AutonomousRuntimeReconcileIntent,
};
use crate::runtime::autonomous_orchestrator::reconcile::reconcile_runtime_snapshot;

const AUTONOMOUS_BOUNDARY_RESUME_EVIDENCE_KIND: &str = "operator_resume";
const AUTONOMOUS_BOUNDARY_RESUME_LABEL: &str = "Operator resumed the blocked boundary.";

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

    if existing_attempt.run_id != existing.run.run_id {
        return Err(CommandError::retryable(
            "autonomous_resume_identity_invalid",
            format!(
                "Cadence cannot resume autonomous boundary `{boundary_id}` because active attempt `{}` belongs to run `{}` instead of durable autonomous run `{}`.",
                existing_attempt.attempt_id, existing_attempt.run_id, existing.run.run_id
            ),
        ));
    }

    let Some(active_boundary_id) = existing_attempt
        .boundary_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(CommandError::retryable(
            "autonomous_resume_identity_invalid",
            format!(
                "Cadence cannot resume autonomous boundary `{boundary_id}` because the active attempt is missing a durable blocked boundary linkage."
            ),
        ));
    };

    if active_boundary_id != boundary_id {
        return Err(CommandError::user_fixable(
            "autonomous_resume_boundary_mismatch",
            format!(
                "Cadence refused to resume boundary `{boundary_id}` because the active autonomous attempt is blocked on boundary `{active_boundary_id}`."
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
