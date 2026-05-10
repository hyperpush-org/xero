use std::path::Path;

use sha2::{Digest, Sha256};

use crate::{
    commands::CommandError,
    db::project_store::{self, AutonomousRunSnapshotRecord},
    runtime::AutonomousSkillSourceMetadata,
};

use super::{
    persist_autonomous_run_scaffold, reconcile_runtime_snapshot, AutonomousRuntimeReconcileIntent,
    AutonomousSkillLifecycleEvent,
};

pub fn persist_skill_lifecycle_event(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    _lifecycle: &AutonomousSkillLifecycleEvent,
) -> Result<Option<AutonomousRunSnapshotRecord>, CommandError> {
    let runtime_snapshot =
        match project_store::load_runtime_run(repo_root, project_id, agent_session_id)? {
            Some(snapshot) => snapshot,
            None => return Ok(None),
        };
    let existing = project_store::load_autonomous_run(repo_root, project_id, agent_session_id)?;
    let payload = reconcile_runtime_snapshot(
        existing.as_ref(),
        &runtime_snapshot,
        AutonomousRuntimeReconcileIntent::Observe,
    );

    persist_autonomous_run_scaffold(repo_root, existing.as_ref(), payload).map(Some)
}

pub(super) fn autonomous_skill_cache_key(source: &AutonomousSkillSourceMetadata) -> String {
    let skill_id = source.path.rsplit('/').next().unwrap_or("skill");
    let mut hasher = Sha256::new();
    hasher.update(format!("{}:{}", source.repo, source.path).as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("{}-{}", skill_id, &digest[..12])
}
