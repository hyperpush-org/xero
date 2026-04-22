pub(crate) use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) use cadence_desktop_lib::{
    commands::{PhaseStatus, PhaseStep, PlanningLifecycleStageKindDto},
    db::{self, database_path_for_repo, project_store},
    git::repository::CanonicalRepository,
    state::ImportFailpoints,
};
pub(crate) use rusqlite::Connection;
pub(crate) use tempfile::TempDir;

pub(crate) fn seed_project(
    root: &TempDir,
    project_id: &str,
    repository_id: &str,
    repo_name: &str,
) -> PathBuf {
    let repo_root = root.path().join(repo_name);
    fs::create_dir_all(&repo_root).expect("create repo root");
    let canonical_root = fs::canonicalize(&repo_root).expect("canonical repo root");

    let repository = CanonicalRepository {
        project_id: project_id.into(),
        repository_id: repository_id.into(),
        root_path: canonical_root.clone(),
        root_path_string: canonical_root.to_string_lossy().into_owned(),
        common_git_dir: canonical_root.join(".git"),
        display_name: repo_name.into(),
        branch_name: Some("main".into()),
        head_sha: Some("abc123".into()),
        branch: None,
        last_commit: None,
        status_entries: Vec::new(),
        has_staged_changes: false,
        has_unstaged_changes: false,
        has_untracked_changes: false,
    };

    db::import_project(&repository, &ImportFailpoints::default()).expect("import project");
    canonical_root
}

pub(crate) fn open_state_connection(repo_root: &Path) -> Connection {
    Connection::open(database_path_for_repo(repo_root)).expect("open state db")
}

pub(crate) fn seed_auto_dispatch_workflow_graph(repo_root: &Path, project_id: &str) {
    project_store::upsert_workflow_graph(
        repo_root,
        project_id,
        &project_store::WorkflowGraphUpsertRecord {
            nodes: vec![
                project_store::WorkflowGraphNodeRecord {
                    node_id: "plan".into(),
                    phase_id: 1,
                    sort_order: 1,
                    name: "Plan".into(),
                    description: "Plan phase".into(),
                    status: PhaseStatus::Active,
                    current_step: Some(PhaseStep::Plan),
                    task_count: 1,
                    completed_tasks: 1,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "execute".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Execute".into(),
                    description: "Execute phase".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Execute),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "verify".into(),
                    phase_id: 3,
                    sort_order: 3,
                    name: "Verify".into(),
                    description: "Verify phase".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Verify),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "plan".into(),
                    to_node_id: "execute".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: Some("execution_gate".into()),
                },
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "execute".into(),
                    to_node_id: "verify".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: None,
                },
            ],
            gates: vec![project_store::WorkflowGateMetadataRecord {
                node_id: "execute".into(),
                gate_key: "execution_gate".into(),
                gate_state: project_store::WorkflowGateState::Pending,
                action_type: Some("approve_execution".into()),
                title: Some("Approve execution".into()),
                detail: Some("Operator approval required.".into()),
                decision_context: None,
            }],
        },
    )
    .expect("seed graph with automatic continuation");
}
