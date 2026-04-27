pub(crate) use std::path::{Path, PathBuf};

pub(crate) use cadence_desktop_lib::{
    db::{self, database_path_for_repo, project_store},
    git::repository::CanonicalRepository,
    state::DesktopState,
};
pub(crate) use rusqlite::{params, Connection};
pub(crate) use tempfile::TempDir;

pub(crate) fn seed_project(
    root: &TempDir,
    project_id: &str,
    repository_id: &str,
    repo_name: &str,
) -> PathBuf {
    let repo_root = root.path().join(repo_name);
    std::fs::create_dir_all(&repo_root).expect("create repo root");
    let canonical_root = std::fs::canonicalize(&repo_root).expect("canonical repo root");
    let root_path_string = canonical_root.to_string_lossy().into_owned();

    let repository = CanonicalRepository {
        project_id: project_id.into(),
        repository_id: repository_id.into(),
        root_path: canonical_root.clone(),
        root_path_string,
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
        additions: 0,
        deletions: 0,
    };

    db::configure_project_database_paths(&root.path().join("app-data").join("cadence.db"));
    let state = DesktopState::default();
    db::import_project(&repository, state.import_failpoints()).expect("import project");

    canonical_root
}

pub(crate) fn open_state_connection(repo_root: &Path) -> Connection {
    Connection::open(database_path_for_repo(repo_root)).expect("open repo-local database")
}

pub(crate) fn sample_run(project_id: &str, run_id: &str) -> project_store::RuntimeRunRecord {
    project_store::RuntimeRunRecord {
        project_id: project_id.into(),
        agent_session_id: "agent-session-main".into(),
        run_id: run_id.into(),
        runtime_kind: "openai_codex".into(),
        provider_id: "openai_codex".into(),
        supervisor_kind: "detached_pty".into(),
        status: project_store::RuntimeRunStatus::Running,
        transport: project_store::RuntimeRunTransportRecord {
            kind: "tcp".into(),
            endpoint: "127.0.0.1:4455".into(),
            liveness: project_store::RuntimeRunTransportLiveness::Unknown,
        },
        started_at: "2026-04-15T19:00:00Z".into(),
        last_heartbeat_at: Some("2099-04-15T19:00:10Z".into()),
        stopped_at: None,
        last_error: None,
        updated_at: "2099-04-15T19:00:10Z".into(),
    }
}

pub(crate) fn sample_control_state(timestamp: &str) -> project_store::RuntimeRunControlStateRecord {
    project_store::build_runtime_run_control_state(
        "openai_codex",
        Some(cadence_desktop_lib::commands::ProviderModelThinkingEffortDto::Medium),
        cadence_desktop_lib::commands::RuntimeRunApprovalModeDto::Suggest,
        timestamp,
        None,
    )
    .expect("build sample runtime run controls")
}

pub(crate) fn sample_checkpoint(
    project_id: &str,
    run_id: &str,
    sequence: u32,
    kind: project_store::RuntimeRunCheckpointKind,
    summary: &str,
    created_at: &str,
) -> project_store::RuntimeRunCheckpointRecord {
    project_store::RuntimeRunCheckpointRecord {
        project_id: project_id.into(),
        run_id: run_id.into(),
        sequence,
        kind,
        summary: summary.into(),
        created_at: created_at.into(),
    }
}

