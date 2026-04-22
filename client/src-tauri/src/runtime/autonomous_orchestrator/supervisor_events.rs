use std::path::Path;

use crate::{
    auth::now_timestamp,
    commands::CommandError,
    db::project_store::{
        self, AutonomousArtifactCommandResultRecord, AutonomousArtifactPayloadRecord,
        AutonomousPolicyDeniedPayloadRecord, AutonomousRunSnapshotRecord, AutonomousRunStatus,
        AutonomousToolCallStateRecord, AutonomousToolResultPayloadRecord,
        AutonomousUnitArtifactRecord, AutonomousUnitArtifactStatus, AutonomousUnitStatus,
        AutonomousVerificationEvidencePayloadRecord, AutonomousVerificationOutcomeRecord,
        RuntimeRunDiagnosticRecord,
    },
    runtime::{
        protocol::{
            CommandToolResultSummary, SupervisorLiveEventPayload, SupervisorSkillCacheStatus,
            SupervisorSkillDiagnostic, SupervisorSkillLifecycleResult,
            SupervisorSkillLifecycleStage, SupervisorToolCallState, ToolResultSummary,
        },
        AutonomousSkillCacheStatus, AutonomousSkillSourceMetadata,
    },
};

use super::{
    existing_artifact_timestamp, persist_progressed_autonomous_run, persist_skill_lifecycle_event,
    upsert_artifact, AutonomousRuntimeReconcileIntent, AutonomousSkillLifecycleEvent,
};
use crate::runtime::autonomous_orchestrator::reconcile::{
    reconcile_runtime_snapshot, AUTONOMOUS_BOUNDARY_PAUSE_CODE,
};

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
        SupervisorLiveEventPayload::Skill {
            skill_id,
            stage,
            result,
            detail: _,
            source,
            cache_status,
            diagnostic,
        } => {
            let source = autonomous_skill_source_metadata_from_supervisor(source);
            let lifecycle = AutonomousSkillLifecycleEvent {
                stage: autonomous_skill_lifecycle_stage_record_from_supervisor(stage),
                result: autonomous_skill_lifecycle_result_record_from_supervisor(result),
                skill_id: skill_id.clone(),
                cache_key: super::skill_lifecycle::autonomous_skill_cache_key(&source),
                source,
                cache_status: cache_status
                    .as_ref()
                    .map(autonomous_skill_cache_status_from_supervisor),
                diagnostic: diagnostic
                    .as_ref()
                    .map(command_error_from_supervisor_skill_diagnostic),
            };
            return persist_skill_lifecycle_event(repo_root, project_id, &lifecycle);
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
                let (action_id, boundary_id) = current_boundary_linkage(existing.as_ref());
                let artifact_suffix = linkage_suffix(boundary_id.as_deref());
                let artifact_id = format!(
                    "{}:policy:{}{}",
                    attempt.attempt_id,
                    sanitize_artifact_fragment(code),
                    artifact_suffix
                );
                let evidence_artifact_id = format!(
                    "{}:verification:{}{}",
                    attempt.attempt_id,
                    sanitize_artifact_fragment(code),
                    artifact_suffix
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
                                action_id: action_id.clone(),
                                boundary_id: boundary_id.clone(),
                            },
                        )),
                        created_at: timestamp.clone(),
                        updated_at: now_timestamp(),
                    },
                );

                let evidence_created_at =
                    existing_artifact_timestamp(existing.as_ref(), &evidence_artifact_id)
                        .unwrap_or_else(|| timestamp.clone());
                upsert_artifact(
                    &mut payload.artifacts,
                    AutonomousUnitArtifactRecord {
                        project_id: attempt.project_id.clone(),
                        run_id: attempt.run_id.clone(),
                        unit_id: attempt.unit_id.clone(),
                        attempt_id: attempt.attempt_id.clone(),
                        artifact_id: evidence_artifact_id.clone(),
                        artifact_kind: "verification_evidence".into(),
                        status: AutonomousUnitArtifactStatus::Recorded,
                        summary: format!(
                            "Autonomous attempt recorded stable policy denial `{code}`."
                        ),
                        content_hash: None,
                        payload: Some(AutonomousArtifactPayloadRecord::VerificationEvidence(
                            AutonomousVerificationEvidencePayloadRecord {
                                project_id: attempt.project_id.clone(),
                                run_id: attempt.run_id.clone(),
                                unit_id: attempt.unit_id.clone(),
                                attempt_id: attempt.attempt_id.clone(),
                                artifact_id: evidence_artifact_id,
                                evidence_kind: code.clone(),
                                label: title.clone(),
                                outcome: AutonomousVerificationOutcomeRecord::Failed,
                                command_result: None,
                                action_id,
                                boundary_id,
                            },
                        )),
                        created_at: evidence_created_at,
                        updated_at: now_timestamp(),
                    },
                );
            }
        }
        _ => return Ok(None),
    }

    persist_progressed_autonomous_run(repo_root, project_id, existing.as_ref(), payload).map(Some)
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

fn autonomous_skill_source_metadata_from_supervisor(
    source: &crate::runtime::protocol::SupervisorSkillSourceMetadata,
) -> AutonomousSkillSourceMetadata {
    AutonomousSkillSourceMetadata {
        repo: source.repo.clone(),
        path: source.path.clone(),
        reference: source.reference.clone(),
        tree_hash: source.tree_hash.clone(),
    }
}

fn autonomous_skill_lifecycle_stage_record_from_supervisor(
    stage: &SupervisorSkillLifecycleStage,
) -> project_store::AutonomousSkillLifecycleStageRecord {
    match stage {
        SupervisorSkillLifecycleStage::Discovery => {
            project_store::AutonomousSkillLifecycleStageRecord::Discovery
        }
        SupervisorSkillLifecycleStage::Install => {
            project_store::AutonomousSkillLifecycleStageRecord::Install
        }
        SupervisorSkillLifecycleStage::Invoke => {
            project_store::AutonomousSkillLifecycleStageRecord::Invoke
        }
    }
}

fn autonomous_skill_lifecycle_result_record_from_supervisor(
    result: &SupervisorSkillLifecycleResult,
) -> project_store::AutonomousSkillLifecycleResultRecord {
    match result {
        SupervisorSkillLifecycleResult::Succeeded => {
            project_store::AutonomousSkillLifecycleResultRecord::Succeeded
        }
        SupervisorSkillLifecycleResult::Failed => {
            project_store::AutonomousSkillLifecycleResultRecord::Failed
        }
    }
}

fn autonomous_skill_cache_status_from_supervisor(
    status: &SupervisorSkillCacheStatus,
) -> AutonomousSkillCacheStatus {
    match status {
        SupervisorSkillCacheStatus::Miss => AutonomousSkillCacheStatus::Miss,
        SupervisorSkillCacheStatus::Hit => AutonomousSkillCacheStatus::Hit,
        SupervisorSkillCacheStatus::Refreshed => AutonomousSkillCacheStatus::Refreshed,
    }
}

fn command_error_from_supervisor_skill_diagnostic(
    diagnostic: &SupervisorSkillDiagnostic,
) -> CommandError {
    if diagnostic.retryable {
        CommandError::retryable(diagnostic.code.clone(), diagnostic.message.clone())
    } else {
        CommandError::user_fixable(diagnostic.code.clone(), diagnostic.message.clone())
    }
}

fn current_boundary_linkage(
    existing: Option<&AutonomousRunSnapshotRecord>,
) -> (Option<String>, Option<String>) {
    let Some(existing) = existing else {
        return (None, None);
    };
    let Some(boundary_id) = existing
        .attempt
        .as_ref()
        .and_then(|attempt| attempt.boundary_id.clone())
    else {
        return (None, None);
    };

    let action_id = existing
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .find_map(|artifact| match artifact.payload.as_ref() {
            Some(AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                if payload.boundary_id.as_deref() == Some(boundary_id.as_str())
                    && payload.outcome == AutonomousVerificationOutcomeRecord::Blocked =>
            {
                payload.action_id.clone()
            }
            _ => None,
        });

    match action_id {
        Some(action_id) => (Some(action_id), Some(boundary_id)),
        None => (None, None),
    }
}

fn linkage_suffix(boundary_id: Option<&str>) -> String {
    boundary_id
        .map(sanitize_artifact_fragment)
        .map(|boundary| format!(":boundary:{boundary}"))
        .unwrap_or_default()
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
