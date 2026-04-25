use std::{path::Path, thread, time::Duration};

use super::autonomous_workflow_progression::persist_autonomous_workflow_progression;

use crate::{
    commands::CommandError,
    db::project_store::{
        AutonomousRunSnapshotRecord, AutonomousRunUpsertRecord,
        AutonomousSkillLifecycleResultRecord, AutonomousSkillLifecycleStageRecord,
        AutonomousUnitArtifactRecord,
    },
    runtime::{
        AutonomousSkillCacheStatus, AutonomousSkillInstallOutput, AutonomousSkillInvokeOutput,
        AutonomousSkillSourceMetadata,
    },
};

mod operator_resume;
mod reconcile;
mod skill_lifecycle;
mod supervisor_events;

pub use operator_resume::{persist_operator_resume, validate_operator_resume_target};
pub use reconcile::reconcile_runtime_snapshot;
pub use skill_lifecycle::persist_skill_lifecycle_event;
pub use supervisor_events::persist_supervisor_event;

const AUTONOMOUS_RUN_PERSIST_MAX_ATTEMPTS: usize = 20;
const AUTONOMOUS_RUN_PERSIST_RETRY_DELAY_MS: u64 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomousRuntimeReconcileIntent {
    Observe,
    DuplicateStart,
    CancelRequested,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousSkillLifecycleEvent {
    pub stage: AutonomousSkillLifecycleStageRecord,
    pub result: AutonomousSkillLifecycleResultRecord,
    pub skill_id: String,
    pub source: AutonomousSkillSourceMetadata,
    pub cache_key: String,
    pub cache_status: Option<AutonomousSkillCacheStatus>,
    pub diagnostic: Option<CommandError>,
}

impl AutonomousSkillLifecycleEvent {
    pub fn discovered(skill_id: impl Into<String>, source: AutonomousSkillSourceMetadata) -> Self {
        Self {
            stage: AutonomousSkillLifecycleStageRecord::Discovery,
            result: AutonomousSkillLifecycleResultRecord::Succeeded,
            skill_id: skill_id.into(),
            cache_key: skill_lifecycle::autonomous_skill_cache_key(&source),
            source,
            cache_status: None,
            diagnostic: None,
        }
    }

    pub fn installed(output: &AutonomousSkillInstallOutput) -> Self {
        Self {
            stage: AutonomousSkillLifecycleStageRecord::Install,
            result: AutonomousSkillLifecycleResultRecord::Succeeded,
            skill_id: output.skill_id.clone(),
            source: output.source.clone(),
            cache_key: output.cache_key.clone(),
            cache_status: Some(output.cache_status),
            diagnostic: None,
        }
    }

    pub fn invoked(output: &AutonomousSkillInvokeOutput) -> Self {
        Self {
            stage: AutonomousSkillLifecycleStageRecord::Invoke,
            result: AutonomousSkillLifecycleResultRecord::Succeeded,
            skill_id: output.skill_id.clone(),
            source: output.source.clone(),
            cache_key: output.cache_key.clone(),
            cache_status: Some(output.cache_status),
            diagnostic: None,
        }
    }

    pub fn failed(
        stage: AutonomousSkillLifecycleStageRecord,
        skill_id: impl Into<String>,
        source: AutonomousSkillSourceMetadata,
        cache_status: Option<AutonomousSkillCacheStatus>,
        diagnostic: &CommandError,
    ) -> Self {
        Self {
            stage,
            result: AutonomousSkillLifecycleResultRecord::Failed,
            skill_id: skill_id.into(),
            cache_key: skill_lifecycle::autonomous_skill_cache_key(&source),
            source,
            cache_status,
            diagnostic: Some(diagnostic.clone()),
        }
    }
}

fn persist_progressed_autonomous_run(
    repo_root: &Path,
    project_id: &str,
    existing: Option<&AutonomousRunSnapshotRecord>,
    payload: AutonomousRunUpsertRecord,
) -> Result<AutonomousRunSnapshotRecord, CommandError> {
    let mut last_retryable_error: Option<CommandError> = None;

    for attempt in 1..=AUTONOMOUS_RUN_PERSIST_MAX_ATTEMPTS {
        match persist_autonomous_workflow_progression(
            repo_root,
            project_id,
            existing,
            payload.clone(),
        ) {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) if error.retryable && attempt < AUTONOMOUS_RUN_PERSIST_MAX_ATTEMPTS => {
                last_retryable_error = Some(error);
                thread::sleep(Duration::from_millis(AUTONOMOUS_RUN_PERSIST_RETRY_DELAY_MS));
            }
            Err(error) => return Err(error),
        }
    }

    Err(last_retryable_error.expect("retry loop should retain last retryable autonomous error"))
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
