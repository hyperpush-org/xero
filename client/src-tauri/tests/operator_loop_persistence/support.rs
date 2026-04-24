pub(crate) use std::path::{Path, PathBuf};

pub(crate) use cadence_desktop_lib::{
    commands::{
        apply_workflow_transition::apply_workflow_transition, get_project_snapshot,
        resolve_operator_action::resolve_operator_action, resume_operator_run::resume_operator_run,
        ApplyWorkflowTransitionRequestDto, OperatorApprovalStatus, ProjectIdRequestDto,
        ResolveOperatorActionRequestDto, ResumeHistoryStatus, ResumeOperatorRunRequestDto,
        VerificationRecordStatus, WorkflowTransitionGateUpdateRequestDto,
    },
    configure_builder_with_state,
    db::{self, database_path_for_repo, project_store},
    git::repository::CanonicalRepository,
    registry::{self, RegistryProjectRecord},
    state::DesktopState,
};
pub(crate) use rusqlite::{params, Connection};
pub(crate) use tauri::Manager;
pub(crate) use tempfile::TempDir;

pub(crate) fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

pub(crate) fn create_state(root: &TempDir) -> (DesktopState, PathBuf) {
    let registry_path = root.path().join("app-data").join("project-registry.json");
    (
        DesktopState::default().with_registry_file_override(registry_path.clone()),
        registry_path,
    )
}

pub(crate) fn seed_project(
    root: &TempDir,
    app: &tauri::App<tauri::test::MockRuntime>,
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
        root_path_string: root_path_string.clone(),
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

    db::import_project(&repository, app.state::<DesktopState>().import_failpoints())
        .expect("import project into repo-local db");

    let registry_path = app
        .state::<DesktopState>()
        .registry_file(&app.handle().clone())
        .expect("registry path");
    registry::replace_projects(
        &registry_path,
        vec![RegistryProjectRecord {
            project_id: repository.project_id.clone(),
            repository_id: repository.repository_id.clone(),
            root_path: root_path_string,
        }],
    )
    .expect("persist registry entry");

    canonical_root
}

pub(crate) fn open_state_connection(repo_root: &Path) -> Connection {
    Connection::open(database_path_for_repo(repo_root)).expect("open repo-local database")
}

pub(crate) fn count_operator_approval_rows_for_action(
    repo_root: &Path,
    project_id: &str,
    action_id: &str,
) -> i64 {
    let connection = open_state_connection(repo_root);
    connection
        .query_row(
            "SELECT COUNT(*) FROM operator_approvals WHERE project_id = ?1 AND action_id = ?2",
            [project_id, action_id],
            |row| row.get(0),
        )
        .expect("count operator approval rows for action")
}

pub(crate) fn insert_operator_loop_rows(repo_root: &Path, project_id: &str) {
    let connection = open_state_connection(repo_root);

    connection
        .execute(
            r#"
            INSERT INTO operator_approvals (
                project_id,
                action_id,
                session_id,
                flow_id,
                action_type,
                title,
                detail,
                status,
                decision_note,
                created_at,
                updated_at,
                resolved_at
            )
            VALUES (?1, 'approve-plan', 'session-1', 'flow-1', 'plan_workflow', 'Project needs a workflow plan', 'Plan the first milestone before interactive execution.', 'pending', NULL, '2026-04-13T20:00:49Z', '2026-04-13T20:00:49Z', NULL)
            "#,
            [project_id],
        )
        .expect("insert pending approval");

    connection
        .execute(
            r#"
            INSERT INTO operator_approvals (
                project_id,
                action_id,
                session_id,
                flow_id,
                action_type,
                title,
                detail,
                status,
                decision_note,
                created_at,
                updated_at,
                resolved_at
            )
            VALUES (?1, 'review-worktree', 'session-1', 'flow-1', 'review_worktree', 'Repository has local changes', 'Review the worktree before trusting subsequent agent actions.', 'approved', 'Changes reviewed and accepted.', '2026-04-13T19:59:10Z', '2026-04-13T20:04:19Z', '2026-04-13T20:04:19Z')
            "#,
            [project_id],
        )
        .expect("insert resolved approval");

    connection
        .execute(
            r#"
            INSERT INTO operator_verification_records (
                project_id,
                source_action_id,
                status,
                summary,
                detail,
                recorded_at
            )
            VALUES (?1, 'review-worktree', 'passed', 'Reviewed repository status before resume.', 'Worktree inspection completed without blocking changes.', '2026-04-13T20:05:12Z')
            "#,
            [project_id],
        )
        .expect("insert verification record");

    connection
        .execute(
            r#"
            INSERT INTO operator_resume_history (
                project_id,
                source_action_id,
                session_id,
                status,
                summary,
                created_at
            )
            VALUES (?1, 'review-worktree', 'session-1', 'started', 'Operator resumed the selected project runtime.', '2026-04-13T20:06:33Z')
            "#,
            [project_id],
        )
        .expect("insert resume history");
}

pub(crate) fn insert_other_project_rows(repo_root: &Path) {
    let connection = open_state_connection(repo_root);
    connection
        .execute(
            r#"
            INSERT INTO projects (
                id,
                name,
                description,
                milestone,
                total_phases,
                completed_phases,
                active_phase,
                branch,
                runtime,
                updated_at
            )
            VALUES ('project-2', 'other-repo', '', '', 0, 0, 0, 'main', NULL, '2026-04-13T19:00:00Z')
            "#,
            [],
        )
        .expect("insert other project");

    connection
        .execute(
            r#"
            INSERT INTO operator_approvals (
                project_id,
                action_id,
                action_type,
                title,
                detail,
                status,
                created_at,
                updated_at,
                resolved_at,
                decision_note
            )
            VALUES ('project-2', 'other-action', 'review_worktree', 'Other project approval', 'Must stay scoped to the second project.', 'rejected', '2026-04-13T19:10:00Z', '2026-04-13T19:11:00Z', '2026-04-13T19:11:00Z', 'Rejected for another repo.')
            "#,
            [],
        )
        .expect("insert other project approval");

    connection
        .execute(
            r#"
            INSERT INTO operator_verification_records (
                project_id,
                source_action_id,
                status,
                summary,
                detail,
                recorded_at
            )
            VALUES ('project-2', 'other-action', 'failed', 'Other project verification', 'Should never appear in project-1 snapshot.', '2026-04-13T19:12:00Z')
            "#,
            [],
        )
        .expect("insert other project verification");

    connection
        .execute(
            r#"
            INSERT INTO operator_resume_history (
                project_id,
                source_action_id,
                session_id,
                status,
                summary,
                created_at
            )
            VALUES ('project-2', 'other-action', 'session-2', 'failed', 'Other project resume failed.', '2026-04-13T19:13:00Z')
            "#,
            [],
        )
        .expect("insert other project resume history");
}

pub(crate) fn seed_gate_linked_workflow(repo_root: &Path, project_id: &str, action_type: &str) {
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
                    status: cadence_desktop_lib::commands::PhaseStatus::Active,
                    current_step: Some(cadence_desktop_lib::commands::PhaseStep::Plan),
                    task_count: 2,
                    completed_tasks: 1,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "execute".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Execute".into(),
                    description: "Execute phase".into(),
                    status: cadence_desktop_lib::commands::PhaseStatus::Pending,
                    current_step: Some(cadence_desktop_lib::commands::PhaseStep::Execute),
                    task_count: 4,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![project_store::WorkflowGraphEdgeRecord {
                from_node_id: "plan".into(),
                to_node_id: "execute".into(),
                transition_kind: "advance".into(),
                gate_requirement: Some("execution_gate".into()),
            }],
            gates: vec![project_store::WorkflowGateMetadataRecord {
                node_id: "execute".into(),
                gate_key: "execution_gate".into(),
                gate_state: project_store::WorkflowGateState::Pending,
                action_type: Some(action_type.into()),
                title: Some("Approve execution".into()),
                detail: Some("Operator approval required.".into()),
                decision_context: None,
            }],
        },
    )
    .expect("seed gate-linked workflow graph");
}

pub(crate) fn seed_gate_linked_workflow_with_auto_continuation(
    repo_root: &Path,
    project_id: &str,
    action_type: &str,
) {
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
                    status: cadence_desktop_lib::commands::PhaseStatus::Active,
                    current_step: Some(cadence_desktop_lib::commands::PhaseStep::Plan),
                    task_count: 2,
                    completed_tasks: 1,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "execute".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Execute".into(),
                    description: "Execute phase".into(),
                    status: cadence_desktop_lib::commands::PhaseStatus::Pending,
                    current_step: Some(cadence_desktop_lib::commands::PhaseStep::Execute),
                    task_count: 4,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "verify".into(),
                    phase_id: 3,
                    sort_order: 3,
                    name: "Verify".into(),
                    description: "Verify phase".into(),
                    status: cadence_desktop_lib::commands::PhaseStatus::Pending,
                    current_step: Some(cadence_desktop_lib::commands::PhaseStep::Verify),
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
                action_type: Some(action_type.into()),
                title: Some("Approve execution".into()),
                detail: Some("Operator approval required.".into()),
                decision_context: None,
            }],
        },
    )
    .expect("seed gate-linked workflow graph with auto continuation");
}

pub(crate) fn create_legacy_state_db(repo_root: &Path, project_id: &str) -> PathBuf {
    let cadence_dir = repo_root.join(".cadence");
    std::fs::create_dir_all(&cadence_dir).expect("create Cadence dir");
    let database_path = cadence_dir.join("state.db");
    let connection = Connection::open(&database_path).expect("open legacy database");

    connection
        .execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                milestone TEXT NOT NULL DEFAULT '',
                total_phases INTEGER NOT NULL DEFAULT 0 CHECK (total_phases >= 0),
                completed_phases INTEGER NOT NULL DEFAULT 0 CHECK (completed_phases >= 0),
                active_phase INTEGER NOT NULL DEFAULT 0 CHECK (active_phase >= 0),
                branch TEXT,
                runtime TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );

            CREATE TABLE IF NOT EXISTS repositories (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                root_path TEXT NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                branch TEXT,
                head_sha TEXT,
                is_git_repo INTEGER NOT NULL DEFAULT 1 CHECK (is_git_repo IN (0, 1)),
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );

            CREATE INDEX IF NOT EXISTS idx_repositories_project_id ON repositories(project_id);
            CREATE INDEX IF NOT EXISTS idx_repositories_root_path ON repositories(root_path);

            CREATE TABLE IF NOT EXISTS workflow_phases (
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                id INTEGER NOT NULL CHECK (id >= 0),
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL,
                current_step TEXT,
                task_count INTEGER NOT NULL DEFAULT 0 CHECK (task_count >= 0),
                completed_tasks INTEGER NOT NULL DEFAULT 0 CHECK (completed_tasks >= 0),
                summary TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                PRIMARY KEY (project_id, id)
            );

            CREATE INDEX IF NOT EXISTS idx_workflow_phases_project_id_id
                ON workflow_phases(project_id, id);

            CREATE TABLE IF NOT EXISTS runtime_sessions (
                project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
                runtime_kind TEXT NOT NULL,
                provider_id TEXT NOT NULL,
                flow_id TEXT,
                session_id TEXT,
                account_id TEXT,
                auth_phase TEXT NOT NULL,
                last_error_code TEXT,
                last_error_message TEXT,
                last_error_retryable INTEGER,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                CHECK (last_error_retryable IS NULL OR last_error_retryable IN (0, 1))
            );

            CREATE INDEX IF NOT EXISTS idx_runtime_sessions_provider_phase
                ON runtime_sessions(provider_id, auth_phase);
            CREATE INDEX IF NOT EXISTS idx_runtime_sessions_account_id
                ON runtime_sessions(account_id);
            "#,
        )
        .expect("create legacy schema");

    connection
        .execute(
            r#"
            INSERT INTO projects (
                id,
                name,
                description,
                milestone,
                total_phases,
                completed_phases,
                active_phase,
                branch,
                runtime,
                updated_at
            )
            VALUES (?1, 'legacy-repo', '', '', 0, 0, 0, 'main', NULL, '2026-04-13T18:00:00Z')
            "#,
            [project_id],
        )
        .expect("insert legacy project");

    connection
        .execute(
            r#"
            INSERT INTO repositories (
                id,
                project_id,
                root_path,
                display_name,
                branch,
                head_sha,
                is_git_repo,
                updated_at
            )
            VALUES ('repo-legacy', ?1, ?2, 'legacy-repo', 'main', 'abc123', 1, '2026-04-13T18:00:00Z')
            "#,
            params![project_id, repo_root.display().to_string()],
        )
        .expect("insert legacy repository");

    database_path
}
