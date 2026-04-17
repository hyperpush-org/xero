use std::path::{Path, PathBuf};

use cadence_desktop_lib::{
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
use rusqlite::{params, Connection};
use tauri::Manager;
use tempfile::TempDir;

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn create_state(root: &TempDir) -> (DesktopState, PathBuf) {
    let registry_path = root.path().join("app-data").join("project-registry.json");
    (
        DesktopState::default().with_registry_file_override(registry_path.clone()),
        registry_path,
    )
}

fn seed_project(
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

fn open_state_connection(repo_root: &Path) -> Connection {
    Connection::open(database_path_for_repo(repo_root)).expect("open repo-local database")
}

fn insert_operator_loop_rows(repo_root: &Path, project_id: &str) {
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

fn insert_other_project_rows(repo_root: &Path) {
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

fn seed_gate_linked_workflow(repo_root: &Path, project_id: &str, action_type: &str) {
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

fn seed_gate_linked_workflow_with_auto_continuation(
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

fn create_legacy_state_db(repo_root: &Path, project_id: &str) -> PathBuf {
    let cadence_dir = repo_root.join(".cadence");
    std::fs::create_dir_all(&cadence_dir).expect("create cadence dir");
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

#[test]
fn project_snapshot_returns_empty_operator_loop_arrays_when_no_rows_exist() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-1";
    seed_project(&root, &app, project_id, "repo-1", "repo");

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("load project snapshot");

    assert!(snapshot.approval_requests.is_empty());
    assert!(snapshot.verification_records.is_empty());
    assert!(snapshot.resume_history.is_empty());
}

#[test]
fn project_snapshot_persists_operator_loop_metadata_across_reopens_in_stable_order() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-1";
    let repo_root = seed_project(&root, &app, project_id, "repo-1", "repo");
    insert_operator_loop_rows(&repo_root, project_id);

    let first = project_store::load_project_snapshot(&repo_root, project_id)
        .expect("load first snapshot")
        .snapshot;
    let reopened = project_store::load_project_snapshot(&repo_root, project_id)
        .expect("load reopened snapshot")
        .snapshot;

    assert_eq!(first, reopened, "snapshot should be durable across reloads");
    assert_eq!(first.approval_requests.len(), 2);
    assert_eq!(first.approval_requests[0].action_id, "approve-plan");
    assert_eq!(
        first.approval_requests[0].status,
        OperatorApprovalStatus::Pending
    );
    assert_eq!(first.approval_requests[1].action_id, "review-worktree");
    assert_eq!(
        first.approval_requests[1].decision_note.as_deref(),
        Some("Changes reviewed and accepted.")
    );
    assert_eq!(first.verification_records.len(), 1);
    assert_eq!(
        first.verification_records[0].summary,
        "Reviewed repository status before resume."
    );
    assert_eq!(first.resume_history.len(), 1);
    assert_eq!(
        first.resume_history[0].summary,
        "Operator resumed the selected project runtime."
    );
}

#[test]
fn project_snapshot_scopes_operator_loop_rows_to_the_selected_project() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-1";
    let repo_root = seed_project(&root, &app, project_id, "repo-1", "repo");
    insert_operator_loop_rows(&repo_root, project_id);
    insert_other_project_rows(&repo_root);

    let snapshot = project_store::load_project_snapshot(&repo_root, project_id)
        .expect("load scoped snapshot")
        .snapshot;

    assert_eq!(snapshot.approval_requests.len(), 2);
    assert!(snapshot
        .approval_requests
        .iter()
        .all(|approval| approval.action_id != "other-action"));
    assert_eq!(snapshot.verification_records.len(), 1);
    assert!(snapshot
        .verification_records
        .iter()
        .all(|record| record.source_action_id.as_deref() != Some("other-action")));
    assert_eq!(snapshot.resume_history.len(), 1);
    assert!(snapshot
        .resume_history
        .iter()
        .all(|entry| entry.source_action_id.as_deref() != Some("other-action")));
}

#[test]
fn malformed_operator_loop_rows_fail_closed_during_snapshot_decode() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-1";
    let repo_root = seed_project(&root, &app, project_id, "repo-1", "repo");
    insert_operator_loop_rows(&repo_root, project_id);

    let connection = open_state_connection(&repo_root);
    connection
        .execute_batch("PRAGMA ignore_check_constraints = 1;")
        .expect("disable check constraints for corruption test");
    connection
        .execute(
            "UPDATE operator_approvals SET status = 'bogus_status' WHERE project_id = ?1 AND action_id = 'approve-plan'",
            [project_id],
        )
        .expect("corrupt approval status");

    let error = project_store::load_project_snapshot(&repo_root, project_id)
        .expect_err("malformed snapshot rows should fail closed");
    assert_eq!(error.code, "operator_approval_decode_failed");
}

#[test]
fn legacy_repo_local_state_is_upgraded_before_selected_project_snapshot_reads() {
    let root = tempfile::tempdir().expect("temp dir");
    let repo_root = root.path().join("legacy-repo");
    std::fs::create_dir_all(&repo_root).expect("create legacy repo root");
    let project_id = "project-legacy";
    let database_path = create_legacy_state_db(&repo_root, project_id);

    let snapshot = project_store::load_project_snapshot(&repo_root, project_id)
        .expect("load upgraded snapshot")
        .snapshot;

    assert!(snapshot.approval_requests.is_empty());
    assert!(snapshot.verification_records.is_empty());
    assert!(snapshot.resume_history.is_empty());

    let connection = Connection::open(&database_path).expect("reopen upgraded database");
    let tables: Vec<String> = connection
        .prepare(
            r#"
            SELECT name
            FROM sqlite_master
            WHERE type = 'table'
              AND name IN ('operator_approvals', 'operator_verification_records', 'operator_resume_history')
            ORDER BY name ASC
            "#,
        )
        .expect("prepare sqlite_master query")
        .query_map([], |row| row.get(0))
        .expect("query sqlite_master")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect upgraded table names");

    assert_eq!(
        tables,
        vec![
            "operator_approvals".to_string(),
            "operator_resume_history".to_string(),
            "operator_verification_records".to_string(),
        ]
    );
}

#[test]
fn legacy_repo_local_state_upgrade_adds_workflow_handoff_package_schema() {
    let root = tempfile::tempdir().expect("temp dir");
    let repo_root = root.path().join("legacy-repo-handoff-schema");
    std::fs::create_dir_all(&repo_root).expect("create legacy repo root");
    let project_id = "project-legacy-handoff";
    let database_path = create_legacy_state_db(&repo_root, project_id);

    project_store::load_project_snapshot(&repo_root, project_id)
        .expect("load upgraded snapshot for handoff schema assertions");

    let connection = Connection::open(&database_path).expect("reopen upgraded database");

    let table_sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'workflow_handoff_packages'",
            [],
            |row| row.get(0),
        )
        .expect("workflow_handoff_packages table should exist after migration");

    assert!(table_sql.contains("UNIQUE (project_id, handoff_transition_id)"));
    assert!(table_sql.contains("FOREIGN KEY (project_id, handoff_transition_id)"));
    assert!(table_sql.contains("CHECK (json_valid(package_payload))"));

    let indexes: Vec<String> = connection
        .prepare(
            r#"
            SELECT name
            FROM sqlite_master
            WHERE type = 'index'
              AND tbl_name = 'workflow_handoff_packages'
            ORDER BY name ASC
            "#,
        )
        .expect("prepare workflow_handoff_packages index query")
        .query_map([], |row| row.get(0))
        .expect("query workflow_handoff_packages indexes")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect workflow_handoff_packages indexes");

    assert!(indexes
        .iter()
        .any(|name| name == "idx_workflow_handoff_packages_project_created"));
    assert!(indexes
        .iter()
        .any(|name| name == "idx_workflow_handoff_packages_project_causal"));
}

#[test]
fn resolve_operator_action_persists_decision_and_verification_rows() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-1";
    let repo_root = seed_project(&root, &app, project_id, "repo-1", "repo");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "review_worktree",
        "Repository has local changes",
        "Review the worktree before trusting subsequent agent actions.",
        "2026-04-13T20:00:49Z",
    )
    .expect("persist pending approval");
    assert_eq!(pending.status, OperatorApprovalStatus::Pending);

    let resolved = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Worktree reviewed and accepted.".into()),
        },
    )
    .expect("resolve operator action");

    assert_eq!(
        resolved.approval_request.status,
        OperatorApprovalStatus::Approved
    );
    assert_eq!(
        resolved.approval_request.decision_note.as_deref(),
        Some("Worktree reviewed and accepted.")
    );
    assert_eq!(
        resolved.verification_record.status,
        VerificationRecordStatus::Passed
    );
    assert_eq!(
        resolved.verification_record.source_action_id.as_deref(),
        Some(pending.action_id.as_str())
    );

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("load updated snapshot");
    assert_eq!(snapshot.approval_requests.len(), 1);
    assert_eq!(
        snapshot.approval_requests[0].status,
        OperatorApprovalStatus::Approved
    );
    assert_eq!(snapshot.verification_records.len(), 1);
    assert_eq!(
        snapshot.verification_records[0].status,
        VerificationRecordStatus::Passed
    );
    assert!(snapshot.resume_history.is_empty());
}

#[test]
fn resume_operator_run_requires_approved_request_and_records_history() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-1";
    let repo_root = seed_project(&root, &app, project_id, "repo-1", "repo");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "review_worktree",
        "Repository has local changes",
        "Review the worktree before trusting subsequent agent actions.",
        "2026-04-13T20:00:49Z",
    )
    .expect("persist pending approval");

    let before_approval = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            user_answer: None,
        },
    )
    .expect_err("resume should require an approved request");
    assert_eq!(
        before_approval.code,
        "operator_resume_requires_approved_action"
    );

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Worktree reviewed and accepted.".into()),
        },
    )
    .expect("approve operator action");

    let resumed = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            user_answer: None,
        },
    )
    .expect("record resume history");

    assert_eq!(
        resumed.approval_request.status,
        OperatorApprovalStatus::Approved
    );
    assert_eq!(resumed.resume_entry.status, ResumeHistoryStatus::Started);
    assert_eq!(
        resumed.resume_entry.source_action_id.as_deref(),
        Some(pending.action_id.as_str())
    );

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("load updated snapshot");
    assert_eq!(snapshot.resume_history.len(), 1);
    assert_eq!(
        snapshot.resume_history[0].status,
        ResumeHistoryStatus::Started
    );
    assert_eq!(snapshot.verification_records.len(), 1);
}

#[test]
fn resolve_operator_action_rejects_wrong_project_and_already_resolved_requests() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-1";
    let repo_root = seed_project(&root, &app, project_id, "repo-1", "repo");
    insert_other_project_rows(&repo_root);

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "review_worktree",
        "Repository has local changes",
        "Review the worktree before trusting subsequent agent actions.",
        "2026-04-13T20:00:49Z",
    )
    .expect("persist pending approval");

    let wrong_project = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: "other-action".into(),
            decision: "reject".into(),
            user_answer: Some("wrong project".into()),
        },
    )
    .expect_err("cross-project request should stay isolated");
    assert_eq!(wrong_project.code, "operator_action_not_found");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "reject".into(),
            user_answer: Some("Rejected after review.".into()),
        },
    )
    .expect("reject operator action");

    let duplicate = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id,
            decision: "approve".into(),
            user_answer: Some("should fail".into()),
        },
    )
    .expect_err("already-resolved request should be rejected");
    assert_eq!(duplicate.code, "operator_action_already_resolved");
}

#[test]
fn runtime_scoped_resume_rejects_conflicting_user_answer_without_persisting_history() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-runtime-resume-conflict-1";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-runtime-resume-conflict-1",
        "repo-runtime-resume-conflict",
    );

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: "run-conflict-1".into(),
                runtime_kind: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: project_store::RuntimeRunStatus::Running,
                transport: project_store::RuntimeRunTransportRecord {
                    kind: "tcp".into(),
                    endpoint: "127.0.0.1:9".into(),
                    liveness: project_store::RuntimeRunTransportLiveness::Reachable,
                },
                started_at: "2026-04-15T21:00:00Z".into(),
                last_heartbeat_at: Some("2026-04-15T21:00:05Z".into()),
                stopped_at: None,
                last_error: None,
                updated_at: "2026-04-15T21:00:05Z".into(),
            },
            checkpoint: None,
        },
    )
    .expect("persist runtime run for conflicting-answer test");

    let persisted = project_store::upsert_runtime_action_required(
        &repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.into(),
            run_id: "run-conflict-1".into(),
            runtime_kind: "openai_codex".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            transport_endpoint: "127.0.0.1:9".into(),
            started_at: "2026-04-15T21:00:00Z".into(),
            last_heartbeat_at: Some("2026-04-15T21:00:05Z".into()),
            last_error: None,
            boundary_id: "boundary-1".into(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
            checkpoint_summary: "Detached runtime blocked on terminal input and is awaiting operator approval.".into(),
            created_at: "2026-04-15T21:00:06Z".into(),
        },
    )
    .expect("persist runtime action-required approval");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: persisted.approval_request.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("approved".into()),
        },
    )
    .expect("approve runtime action");

    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: persisted.approval_request.action_id,
            user_answer: Some("conflicting answer".into()),
        },
    )
    .expect_err("resume should reject conflicting runtime user answers");
    assert_eq!(error.code, "operator_resume_answer_conflict");

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("load project snapshot after conflicting resume attempt");
    assert!(snapshot.resume_history.is_empty());
    assert_eq!(snapshot.approval_requests.len(), 1);
    assert_eq!(
        snapshot.approval_requests[0].status,
        OperatorApprovalStatus::Approved
    );
}

#[test]
fn runtime_scoped_resume_rejects_corrupted_approved_answer_metadata_without_persisting_history() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-runtime-resume-metadata-conflict-1";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-runtime-resume-metadata-conflict-1",
        "repo-runtime-resume-metadata-conflict",
    );

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: "run-metadata-conflict-1".into(),
                runtime_kind: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: project_store::RuntimeRunStatus::Running,
                transport: project_store::RuntimeRunTransportRecord {
                    kind: "tcp".into(),
                    endpoint: "127.0.0.1:9".into(),
                    liveness: project_store::RuntimeRunTransportLiveness::Reachable,
                },
                started_at: "2026-04-15T21:06:00Z".into(),
                last_heartbeat_at: Some("2026-04-15T21:06:05Z".into()),
                stopped_at: None,
                last_error: None,
                updated_at: "2026-04-15T21:06:05Z".into(),
            },
            checkpoint: None,
        },
    )
    .expect("persist runtime run for metadata-conflict test");

    let persisted = project_store::upsert_runtime_action_required(
        &repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.into(),
            run_id: "run-metadata-conflict-1".into(),
            runtime_kind: "openai_codex".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            transport_endpoint: "127.0.0.1:9".into(),
            started_at: "2026-04-15T21:06:00Z".into(),
            last_heartbeat_at: Some("2026-04-15T21:06:05Z".into()),
            last_error: None,
            boundary_id: "boundary-1".into(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
            checkpoint_summary:
                "Detached runtime blocked on terminal input and is awaiting operator approval."
                    .into(),
            created_at: "2026-04-15T21:06:06Z".into(),
        },
    )
    .expect("persist runtime action-required approval");

    let action_id = persisted.approval_request.action_id.clone();
    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("approved".into()),
        },
    )
    .expect("approve runtime action");

    let connection = open_state_connection(&repo_root);
    connection
        .execute(
            "UPDATE operator_approvals SET decision_note = 'tampered approved answer' WHERE project_id = ?1 AND action_id = ?2",
            params![project_id, action_id.as_str()],
        )
        .expect("corrupt approved decision metadata");

    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id,
            user_answer: None,
        },
    )
    .expect_err("resume should fail closed when approved answer metadata is inconsistent");
    assert_eq!(error.code, "operator_resume_answer_conflict");

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("load project snapshot after metadata-conflict resume failure");
    assert!(snapshot.resume_history.is_empty());
    assert_eq!(snapshot.approval_requests.len(), 1);
    assert_eq!(
        snapshot.approval_requests[0].status,
        OperatorApprovalStatus::Approved
    );
    assert_eq!(snapshot.verification_records.len(), 1);
}

#[test]
fn runtime_scoped_approval_requires_non_secret_user_answer_at_resolve_time() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-runtime-resolve-answer-1";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-runtime-resolve-answer-1",
        "repo-runtime-resolve-answer",
    );

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: "run-require-answer-1".into(),
                runtime_kind: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: project_store::RuntimeRunStatus::Running,
                transport: project_store::RuntimeRunTransportRecord {
                    kind: "tcp".into(),
                    endpoint: "127.0.0.1:9".into(),
                    liveness: project_store::RuntimeRunTransportLiveness::Reachable,
                },
                started_at: "2026-04-15T21:10:00Z".into(),
                last_heartbeat_at: Some("2026-04-15T21:10:05Z".into()),
                stopped_at: None,
                last_error: None,
                updated_at: "2026-04-15T21:10:05Z".into(),
            },
            checkpoint: None,
        },
    )
    .expect("persist runtime run for resolve-answer test");

    let persisted = project_store::upsert_runtime_action_required(
        &repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.into(),
            run_id: "run-require-answer-1".into(),
            runtime_kind: "openai_codex".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            transport_endpoint: "127.0.0.1:9".into(),
            started_at: "2026-04-15T21:10:00Z".into(),
            last_heartbeat_at: Some("2026-04-15T21:10:05Z".into()),
            last_error: None,
            boundary_id: "boundary-1".into(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
            checkpoint_summary:
                "Detached runtime blocked on terminal input and is awaiting operator approval."
                    .into(),
            created_at: "2026-04-15T21:10:06Z".into(),
        },
    )
    .expect("persist runtime action-required approval");

    let action_id = persisted.approval_request.action_id.clone();

    let missing_answer = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: action_id.clone(),
            decision: "approve".into(),
            user_answer: None,
        },
    )
    .expect_err("runtime-scoped approvals should require a recorded answer");
    assert_eq!(missing_answer.code, "operator_action_answer_required");

    let snapshot_after_missing = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("load project snapshot after missing-answer failure");
    assert_eq!(snapshot_after_missing.approval_requests.len(), 1);
    assert_eq!(
        snapshot_after_missing.approval_requests[0].status,
        OperatorApprovalStatus::Pending
    );
    assert!(snapshot_after_missing.verification_records.is_empty());

    let secret_answer = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("oauth access_token=sk-live-secret".into()),
        },
    )
    .expect_err("secret-bearing answer payload should fail closed");
    assert_eq!(
        secret_answer.code,
        "operator_action_decision_payload_invalid"
    );

    let resolved = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("approved".into()),
        },
    )
    .expect("approve runtime-scoped action with a non-empty answer");
    assert_eq!(
        resolved.approval_request.status,
        OperatorApprovalStatus::Approved
    );
    assert_eq!(
        resolved.approval_request.decision_note.as_deref(),
        Some("approved")
    );
    assert_eq!(
        resolved.verification_record.status,
        VerificationRecordStatus::Passed
    );

    let prepared_resume = project_store::prepare_runtime_operator_run_resume(
        &repo_root, project_id, &action_id, None,
    )
    .expect("prepare runtime resume payload after successful approval")
    .expect("runtime-scoped approval should decode into resume payload");
    assert_eq!(prepared_resume.user_answer, "approved");

    let final_snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("load project snapshot after successful runtime approval");
    assert_eq!(final_snapshot.resume_history.len(), 0);
    assert_eq!(final_snapshot.verification_records.len(), 1);
}

#[test]
fn runtime_scoped_approval_rejects_malformed_runtime_identity_without_partial_writes() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-runtime-resolve-malformed-1";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-runtime-resolve-malformed-1",
        "repo-runtime-resolve-malformed",
    );

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: "run-malformed-1".into(),
                runtime_kind: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: project_store::RuntimeRunStatus::Running,
                transport: project_store::RuntimeRunTransportRecord {
                    kind: "tcp".into(),
                    endpoint: "127.0.0.1:9".into(),
                    liveness: project_store::RuntimeRunTransportLiveness::Reachable,
                },
                started_at: "2026-04-15T21:20:00Z".into(),
                last_heartbeat_at: Some("2026-04-15T21:20:05Z".into()),
                stopped_at: None,
                last_error: None,
                updated_at: "2026-04-15T21:20:05Z".into(),
            },
            checkpoint: None,
        },
    )
    .expect("persist runtime run for malformed-identity test");

    let persisted = project_store::upsert_runtime_action_required(
        &repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.into(),
            run_id: "run-malformed-1".into(),
            runtime_kind: "openai_codex".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            transport_endpoint: "127.0.0.1:9".into(),
            started_at: "2026-04-15T21:20:00Z".into(),
            last_heartbeat_at: Some("2026-04-15T21:20:05Z".into()),
            last_error: None,
            boundary_id: "boundary-1".into(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
            checkpoint_summary:
                "Detached runtime blocked on terminal input and is awaiting operator approval."
                    .into(),
            created_at: "2026-04-15T21:20:06Z".into(),
        },
    )
    .expect("persist runtime action-required approval");

    let action_id = persisted.approval_request.action_id.clone();
    let connection = open_state_connection(&repo_root);
    connection
        .execute(
            "UPDATE operator_approvals SET action_type = 'terminal_input' WHERE project_id = ?1 AND action_id = ?2",
            params![project_id, action_id.as_str()],
        )
        .expect("corrupt runtime action identity");

    let malformed = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("approved".into()),
        },
    )
    .expect_err("malformed runtime action identity should fail closed at resolve time");
    assert_eq!(malformed.code, "operator_action_runtime_identity_invalid");

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("load project snapshot after malformed runtime resolve failure");
    assert_eq!(snapshot.approval_requests.len(), 1);
    assert_eq!(
        snapshot.approval_requests[0].status,
        OperatorApprovalStatus::Pending
    );
    assert!(snapshot.verification_records.is_empty());
    assert!(snapshot.resume_history.is_empty());
}

#[test]
fn gate_linked_resume_applies_transition_and_records_causal_event() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-gate-resume-1";
    let repo_root = seed_project(&root, &app, project_id, "repo-gate-1", "repo-gate");
    seed_gate_linked_workflow(&repo_root, project_id, "approve_execution");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-15T19:00:00Z",
    )
    .expect("persist gate-linked approval");

    assert_eq!(pending.gate_node_id.as_deref(), Some("execute"));
    assert_eq!(pending.gate_key.as_deref(), Some("execution_gate"));
    assert_eq!(pending.transition_from_node_id.as_deref(), Some("plan"));
    assert_eq!(pending.transition_to_node_id.as_deref(), Some("execute"));
    assert_eq!(pending.transition_kind.as_deref(), Some("advance"));
    assert!(pending.action_id.contains(":gate:execute:execution_gate:"));

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Execution gate approved by operator.".into()),
        },
    )
    .expect("approve gate-linked operator action");

    let resumed = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            user_answer: None,
        },
    )
    .expect("resume should apply gate-linked transition");

    assert_eq!(resumed.resume_entry.status, ResumeHistoryStatus::Started);

    let graph_after = project_store::load_workflow_graph(&repo_root, project_id)
        .expect("load workflow graph after resume transition");
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "plan")
            .expect("plan node")
            .status,
        cadence_desktop_lib::commands::PhaseStatus::Complete
    );
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "execute")
            .expect("execute node")
            .status,
        cadence_desktop_lib::commands::PhaseStatus::Active
    );

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].from_node_id, "plan");
    assert_eq!(events[0].to_node_id, "execute");
    assert_eq!(events[0].transition_kind, "advance");
    assert_eq!(
        events[0].gate_decision,
        project_store::WorkflowTransitionGateDecision::Approved
    );
    assert_eq!(
        events[0].gate_decision_context.as_deref(),
        Some("Execution gate approved by operator.")
    );
}

#[test]
fn gate_linked_resume_auto_dispatches_next_legal_edge_and_replays_idempotently() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-gate-resume-auto-1";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-gate-resume-auto-1",
        "repo-gate-auto",
    );
    seed_gate_linked_workflow_with_auto_continuation(&repo_root, project_id, "approve_execution");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-15T19:00:00Z",
    )
    .expect("persist gate-linked approval");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Execution gate approved by operator.".into()),
        },
    )
    .expect("approve gate-linked operator action");

    let first_resume = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            user_answer: None,
        },
    )
    .expect("first resume should apply transition and auto-dispatch continuation");
    assert_eq!(
        first_resume.resume_entry.status,
        ResumeHistoryStatus::Started
    );

    let graph_after_first = project_store::load_workflow_graph(&repo_root, project_id)
        .expect("load workflow graph after first resume");
    assert_eq!(
        graph_after_first
            .nodes
            .iter()
            .find(|node| node.node_id == "plan")
            .expect("plan node")
            .status,
        cadence_desktop_lib::commands::PhaseStatus::Complete
    );
    assert_eq!(
        graph_after_first
            .nodes
            .iter()
            .find(|node| node.node_id == "execute")
            .expect("execute node")
            .status,
        cadence_desktop_lib::commands::PhaseStatus::Complete
    );
    assert_eq!(
        graph_after_first
            .nodes
            .iter()
            .find(|node| node.node_id == "verify")
            .expect("verify node")
            .status,
        cadence_desktop_lib::commands::PhaseStatus::Active
    );

    let first_events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after first resume");
    assert_eq!(first_events.len(), 2);

    let primary_event = first_events
        .iter()
        .find(|event| event.from_node_id == "plan" && event.to_node_id == "execute")
        .expect("primary gate-linked transition event");
    let auto_event = first_events
        .iter()
        .find(|event| event.from_node_id == "execute" && event.to_node_id == "verify")
        .expect("automatic continuation transition event");

    assert!(
        primary_event.transition_id.starts_with("resume:"),
        "expected deterministic resume transition id, got {}",
        primary_event.transition_id
    );
    assert!(
        auto_event.transition_id.starts_with("auto:"),
        "expected deterministic auto transition id, got {}",
        auto_event.transition_id
    );
    assert_eq!(
        auto_event.causal_transition_id.as_deref(),
        Some(primary_event.transition_id.as_str())
    );
    assert_eq!(
        auto_event.gate_decision,
        project_store::WorkflowTransitionGateDecision::NotApplicable
    );

    let persisted_auto_package = project_store::load_workflow_handoff_package(
        &repo_root,
        project_id,
        &auto_event.transition_id,
    )
    .expect("load persisted handoff package for auto transition")
    .expect("handoff package row should exist for auto transition");
    let persisted_payload: serde_json::Value =
        serde_json::from_str(&persisted_auto_package.package_payload)
            .expect("decode persisted auto handoff payload");
    assert_eq!(
        persisted_payload["triggerTransition"]["transitionId"],
        auto_event.transition_id
    );
    assert_eq!(
        persisted_payload["triggerTransition"]["causalTransitionId"],
        primary_event.transition_id
    );

    let first_events_reloaded =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("reload transition events after first resume");
    assert_eq!(first_events, first_events_reloaded);

    let replay_resume = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id,
            user_answer: None,
        },
    )
    .expect("replayed resume should be idempotent for transition persistence");
    assert_eq!(
        replay_resume.resume_entry.status,
        ResumeHistoryStatus::Started
    );

    let replay_events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after replayed resume");
    assert_eq!(replay_events.len(), 2);
    assert_eq!(first_events, replay_events);

    let replayed_auto_package = project_store::load_workflow_handoff_package(
        &repo_root,
        project_id,
        &auto_event.transition_id,
    )
    .expect("load replayed handoff package for auto transition")
    .expect("handoff package row should remain persisted across replay");
    assert_eq!(
        persisted_auto_package.package_hash,
        replayed_auto_package.package_hash
    );
}

#[test]
fn command_and_gate_linked_resume_persist_equivalent_transition_shapes() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);

    let command_project_id = "project-gate-parity-command";
    let command_repo_root = seed_project(
        &root,
        &app,
        command_project_id,
        "repo-gate-parity-command",
        "repo-gate-parity-command",
    );
    seed_gate_linked_workflow(&command_repo_root, command_project_id, "approve_execution");

    let command_transition = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ApplyWorkflowTransitionRequestDto {
            project_id: command_project_id.into(),
            transition_id: "transition-parity-command-1".into(),
            causal_transition_id: None,
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_decision: "approved".into(),
            gate_decision_context: Some("approved by operator".into()),
            gate_updates: vec![WorkflowTransitionGateUpdateRequestDto {
                gate_key: "execution_gate".into(),
                gate_state: "satisfied".into(),
                decision_context: Some("approved by operator".into()),
            }],
            occurred_at: "2026-04-15T19:50:00Z".into(),
        },
    )
    .expect("command transition should persist");

    assert_eq!(
        command_transition.transition_event.gate_decision,
        cadence_desktop_lib::commands::WorkflowTransitionGateDecisionDto::Approved
    );

    let resume_project_id = "project-gate-parity-resume";
    let resume_repo_root = seed_project(
        &root,
        &app,
        resume_project_id,
        "repo-gate-parity-resume",
        "repo-gate-parity-resume",
    );
    seed_gate_linked_workflow(&resume_repo_root, resume_project_id, "approve_execution");

    let pending = project_store::upsert_pending_operator_approval(
        &resume_repo_root,
        resume_project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-15T19:50:00Z",
    )
    .expect("persist gate-linked approval for parity test");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: resume_project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("approved by operator".into()),
        },
    )
    .expect("approve gate-linked action for parity test");

    resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: resume_project_id.into(),
            action_id: pending.action_id,
            user_answer: None,
        },
    )
    .expect("resume transition should persist");

    let resume_events = project_store::load_recent_workflow_transition_events(
        &resume_repo_root,
        resume_project_id,
        None,
    )
    .expect("load resume transition events");
    assert_eq!(resume_events.len(), 1);
    let resume_event = &resume_events[0];

    assert_eq!(
        command_transition.transition_event.from_node_id,
        resume_event.from_node_id
    );
    assert_eq!(
        command_transition.transition_event.to_node_id,
        resume_event.to_node_id
    );
    assert_eq!(
        command_transition.transition_event.transition_kind,
        resume_event.transition_kind
    );
    assert_eq!(
        command_transition.transition_event.gate_decision_context,
        resume_event.gate_decision_context
    );
    assert_eq!(
        resume_event.gate_decision,
        project_store::WorkflowTransitionGateDecision::Approved
    );
}

#[test]
fn gate_linked_resume_rejects_illegal_edge_without_side_effects() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-gate-resume-illegal-edge";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-gate-resume-illegal-edge",
        "repo-gate-illegal-edge",
    );
    seed_gate_linked_workflow(&repo_root, project_id, "approve_execution");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-15T20:00:00Z",
    )
    .expect("persist gate-linked approval");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Execution gate approved by operator.".into()),
        },
    )
    .expect("approve gate-linked action");

    let connection = open_state_connection(&repo_root);
    connection
        .execute(
            r#"
            DELETE FROM workflow_graph_edges
            WHERE project_id = ?1
              AND from_node_id = 'plan'
              AND to_node_id = 'execute'
              AND transition_kind = 'advance'
              AND gate_requirement = 'execution_gate'
            "#,
            [project_id],
        )
        .expect("remove legal edge to force illegal-edge failure");

    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id,
            user_answer: None,
        },
    )
    .expect_err("resume should fail when gate-linked edge is missing");
    assert_eq!(error.code, "workflow_transition_illegal_edge");

    let graph_after = project_store::load_workflow_graph(&repo_root, project_id)
        .expect("load workflow graph after illegal-edge failure");
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "plan")
            .expect("plan node")
            .status,
        cadence_desktop_lib::commands::PhaseStatus::Active
    );
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "execute")
            .expect("execute node")
            .status,
        cadence_desktop_lib::commands::PhaseStatus::Pending
    );

    let connection = open_state_connection(&repo_root);
    let resume_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM operator_resume_history WHERE project_id = ?1",
            [project_id],
            |row| row.get(0),
        )
        .expect("count resume rows after failed illegal-edge resume");
    assert_eq!(resume_count, 0);

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after illegal-edge resume failure");
    assert!(events.is_empty());
}

#[test]
fn gate_linked_resume_rejects_unresolved_target_gates_without_side_effects() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-gate-resume-gate-unmet";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-gate-resume-gate-unmet",
        "repo-gate-gate-unmet",
    );
    seed_gate_linked_workflow(&repo_root, project_id, "approve_execution");

    let connection = open_state_connection(&repo_root);
    connection
        .execute(
            r#"
            INSERT INTO workflow_gate_metadata (
                project_id,
                node_id,
                gate_key,
                gate_state,
                action_type,
                title,
                detail,
                decision_context,
                updated_at
            )
            VALUES (?1, 'execute', 'safety_gate', 'pending', 'approve_safety', 'Approve safety', 'Safety review required.', NULL, '2026-04-15T20:10:00Z')
            "#,
            [project_id],
        )
        .expect("insert additional unresolved gate");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-15T20:10:00Z",
    )
    .expect("persist gate-linked approval");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Execution gate approved by operator.".into()),
        },
    )
    .expect("approve gate-linked action");

    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id,
            user_answer: None,
        },
    )
    .expect_err("resume should fail while other target gates remain unresolved");
    assert_eq!(error.code, "workflow_transition_gate_unmet");

    let graph_after = project_store::load_workflow_graph(&repo_root, project_id)
        .expect("load workflow graph after gate-unmet failure");
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "plan")
            .expect("plan node")
            .status,
        cadence_desktop_lib::commands::PhaseStatus::Active
    );
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "execute")
            .expect("execute node")
            .status,
        cadence_desktop_lib::commands::PhaseStatus::Pending
    );

    let execute_gate = graph_after
        .gates
        .iter()
        .find(|gate| gate.gate_key == "execution_gate")
        .expect("execution gate metadata");
    assert_eq!(
        execute_gate.gate_state,
        project_store::WorkflowGateState::Pending
    );

    let safety_gate = graph_after
        .gates
        .iter()
        .find(|gate| gate.gate_key == "safety_gate")
        .expect("safety gate metadata");
    assert_eq!(
        safety_gate.gate_state,
        project_store::WorkflowGateState::Pending
    );

    let connection = open_state_connection(&repo_root);
    let resume_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM operator_resume_history WHERE project_id = ?1",
            [project_id],
            |row| row.get(0),
        )
        .expect("count resume rows after failed gate-unmet resume");
    assert_eq!(resume_count, 0);

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after gate-unmet resume failure");
    assert!(events.is_empty());
}

#[test]
fn gate_linked_resume_rejects_secret_user_answer_input_without_side_effects() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-gate-resume-secret-input";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-gate-resume-secret-input",
        "repo-gate-secret-input",
    );
    seed_gate_linked_workflow(&repo_root, project_id, "approve_execution");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-15T20:20:00Z",
    )
    .expect("persist gate-linked approval");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Execution gate approved by operator.".into()),
        },
    )
    .expect("approve gate-linked action");

    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id,
            user_answer: Some("oauth access_token=sk-live-secret".into()),
        },
    )
    .expect_err("secret-bearing userAnswer should be rejected at resume boundary");
    assert_eq!(error.code, "operator_resume_decision_payload_invalid");

    let connection = open_state_connection(&repo_root);
    let resume_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM operator_resume_history WHERE project_id = ?1",
            [project_id],
            |row| row.get(0),
        )
        .expect("count resume rows after failed secret-input resume");
    assert_eq!(resume_count, 0);

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after failed secret-input resume");
    assert!(events.is_empty());
}

#[test]
fn gate_linked_resume_rejects_missing_transition_context_without_side_effects() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-gate-resume-2";
    let repo_root = seed_project(&root, &app, project_id, "repo-gate-2", "repo-gate");
    seed_gate_linked_workflow(&repo_root, project_id, "approve_execution");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-15T19:10:00Z",
    )
    .expect("persist gate-linked approval");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Execution gate approved by operator.".into()),
        },
    )
    .expect("approve gate-linked action");

    let connection = open_state_connection(&repo_root);
    connection
        .execute_batch("PRAGMA ignore_check_constraints = 1;")
        .expect("disable constraints for corruption test");
    connection
        .execute(
            "UPDATE operator_approvals SET transition_kind = NULL WHERE project_id = ?1 AND action_id = ?2",
            params![project_id, pending.action_id.as_str()],
        )
        .expect("corrupt continuation metadata");

    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id,
            user_answer: None,
        },
    )
    .expect_err("resume should fail closed when continuation metadata is corrupted");
    assert_eq!(error.code, "operator_approval_decode_failed");

    let connection = open_state_connection(&repo_root);
    let resume_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM operator_resume_history WHERE project_id = ?1",
            [project_id],
            |row| row.get(0),
        )
        .expect("count resume rows after failed resume");
    assert_eq!(resume_count, 0);

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after failed resume");
    assert!(events.is_empty());
}

#[test]
fn gate_linked_approval_requires_non_secret_user_answer() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-gate-resume-3";
    let repo_root = seed_project(&root, &app, project_id, "repo-gate-3", "repo-gate");
    seed_gate_linked_workflow(&repo_root, project_id, "approve_execution");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-15T19:20:00Z",
    )
    .expect("persist gate-linked approval");

    let missing_answer = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: None,
        },
    )
    .expect_err("gate-linked approvals should require a recorded answer");
    assert_eq!(missing_answer.code, "operator_action_answer_required");

    let secret_answer = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id,
            decision: "approve".into(),
            user_answer: Some("oauth access_token=sk-live-secret".into()),
        },
    )
    .expect_err("secret-bearing answer payload should fail closed");
    assert_eq!(
        secret_answer.code,
        "operator_action_decision_payload_invalid"
    );
}

#[test]
fn gate_linked_upsert_rejects_ambiguous_gate_context() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-gate-ambiguous-1";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-gate-ambiguous-1",
        "repo-gate",
    );

    project_store::upsert_workflow_graph(
        &repo_root,
        project_id,
        &project_store::WorkflowGraphUpsertRecord {
            nodes: vec![
                project_store::WorkflowGraphNodeRecord {
                    node_id: "a".into(),
                    phase_id: 1,
                    sort_order: 1,
                    name: "A".into(),
                    description: "A".into(),
                    status: cadence_desktop_lib::commands::PhaseStatus::Pending,
                    current_step: Some(cadence_desktop_lib::commands::PhaseStep::Plan),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "b".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "B".into(),
                    description: "B".into(),
                    status: cadence_desktop_lib::commands::PhaseStatus::Pending,
                    current_step: Some(cadence_desktop_lib::commands::PhaseStep::Execute),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "a".into(),
                    to_node_id: "b".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: Some("shared_gate".into()),
                },
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "b".into(),
                    to_node_id: "a".into(),
                    transition_kind: "rollback".into(),
                    gate_requirement: Some("shared_gate".into()),
                },
            ],
            gates: vec![
                project_store::WorkflowGateMetadataRecord {
                    node_id: "a".into(),
                    gate_key: "shared_gate".into(),
                    gate_state: project_store::WorkflowGateState::Pending,
                    action_type: Some("approve_shared".into()),
                    title: Some("Approve shared gate".into()),
                    detail: Some("Multiple unresolved gates share this action.".into()),
                    decision_context: None,
                },
                project_store::WorkflowGateMetadataRecord {
                    node_id: "b".into(),
                    gate_key: "shared_gate".into(),
                    gate_state: project_store::WorkflowGateState::Pending,
                    action_type: Some("approve_shared".into()),
                    title: Some("Approve shared gate".into()),
                    detail: Some("Multiple unresolved gates share this action.".into()),
                    decision_context: None,
                },
            ],
        },
    )
    .expect("seed ambiguous workflow graph");

    let error = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_shared",
        "Approve shared gate",
        "Multiple unresolved gates share this action.",
        "2026-04-15T19:30:00Z",
    )
    .expect_err("ambiguous unresolved gates should fail closed");

    assert_eq!(error.code, "operator_approval_gate_ambiguous");
}

#[test]
fn repeated_action_type_uses_gate_scoped_action_ids() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-gate-repeat-1";
    let repo_root = seed_project(&root, &app, project_id, "repo-gate-repeat-1", "repo-gate");

    project_store::upsert_workflow_graph(
        &repo_root,
        project_id,
        &project_store::WorkflowGraphUpsertRecord {
            nodes: vec![
                project_store::WorkflowGraphNodeRecord {
                    node_id: "plan".into(),
                    phase_id: 1,
                    sort_order: 1,
                    name: "Plan".into(),
                    description: "Plan".into(),
                    status: cadence_desktop_lib::commands::PhaseStatus::Complete,
                    current_step: Some(cadence_desktop_lib::commands::PhaseStep::Plan),
                    task_count: 1,
                    completed_tasks: 1,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "execute".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Execute".into(),
                    description: "Execute".into(),
                    status: cadence_desktop_lib::commands::PhaseStatus::Active,
                    current_step: Some(cadence_desktop_lib::commands::PhaseStep::Execute),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "verify".into(),
                    phase_id: 3,
                    sort_order: 3,
                    name: "Verify".into(),
                    description: "Verify".into(),
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
                    gate_requirement: Some("execute_gate".into()),
                },
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "execute".into(),
                    to_node_id: "verify".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: Some("verify_gate".into()),
                },
            ],
            gates: vec![
                project_store::WorkflowGateMetadataRecord {
                    node_id: "execute".into(),
                    gate_key: "execute_gate".into(),
                    gate_state: project_store::WorkflowGateState::Pending,
                    action_type: Some("approve_stage".into()),
                    title: Some("Approve execute stage".into()),
                    detail: Some("Approve execute gate".into()),
                    decision_context: None,
                },
                project_store::WorkflowGateMetadataRecord {
                    node_id: "verify".into(),
                    gate_key: "verify_gate".into(),
                    gate_state: project_store::WorkflowGateState::Pending,
                    action_type: Some("approve_stage".into()),
                    title: Some("Approve verify stage".into()),
                    detail: Some("Approve verify gate".into()),
                    decision_context: None,
                },
            ],
        },
    )
    .expect("seed repeated-action graph");

    let first = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_stage",
        "Approve execute stage",
        "Approve execute gate",
        "2026-04-15T19:40:00Z",
    )
    .expect("persist first gate-scoped approval");

    let connection = open_state_connection(&repo_root);
    connection
        .execute(
            "UPDATE workflow_graph_nodes SET status = 'complete' WHERE project_id = ?1 AND node_id = 'execute'",
            [project_id],
        )
        .expect("complete execute node");
    connection
        .execute(
            "UPDATE workflow_graph_nodes SET status = 'active' WHERE project_id = ?1 AND node_id = 'verify'",
            [project_id],
        )
        .expect("activate verify node");
    connection
        .execute(
            "UPDATE workflow_gate_metadata SET gate_state = 'satisfied' WHERE project_id = ?1 AND node_id = 'execute' AND gate_key = 'execute_gate'",
            [project_id],
        )
        .expect("satisfy execute gate");

    let second = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_stage",
        "Approve verify stage",
        "Approve verify gate",
        "2026-04-15T19:41:00Z",
    )
    .expect("persist second gate-scoped approval");

    assert_ne!(first.action_id, second.action_id);
    assert_eq!(first.gate_key.as_deref(), Some("execute_gate"));
    assert_eq!(second.gate_key.as_deref(), Some("verify_gate"));
}

#[test]
fn notification_dispatch_claim_flow_is_idempotent_for_pending_operator_approvals() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-notification-loop-1";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-notification-loop-1",
        "repo-notification-loop",
    );

    project_store::upsert_notification_route(
        &repo_root,
        &project_store::NotificationRouteUpsertRecord {
            project_id: project_id.into(),
            route_id: "route-discord".into(),
            route_kind: "discord".into(),
            route_target: "discord:ops-room".into(),
            enabled: true,
            metadata_json: Some("{\"label\":\"ops\"}".into()),
            updated_at: "2026-04-16T20:00:00Z".into(),
        },
    )
    .expect("persist notification route");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "terminal_input_required",
        "Terminal input required",
        "Runtime paused and requires a coarse operator answer.",
        "2026-04-16T20:00:01Z",
    )
    .expect("persist pending approval");

    let first = project_store::enqueue_notification_dispatches(
        &repo_root,
        &project_store::NotificationDispatchEnqueueRecord {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            enqueued_at: "2026-04-16T20:00:02Z".into(),
        },
    )
    .expect("enqueue notification dispatch");
    let second = project_store::enqueue_notification_dispatches(
        &repo_root,
        &project_store::NotificationDispatchEnqueueRecord {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            enqueued_at: "2026-04-16T20:00:03Z".into(),
        },
    )
    .expect("re-enqueue notification dispatch");

    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_eq!(first[0].id, second[0].id);

    let claim = project_store::claim_notification_reply(
        &repo_root,
        &project_store::NotificationReplyClaimRequestRecord {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            route_id: second[0].route_id.clone(),
            correlation_key: second[0].correlation_key.clone(),
            responder_id: Some("operator-a".into()),
            reply_text: "approved".into(),
            received_at: "2026-04-16T20:00:04Z".into(),
        },
    )
    .expect("first reply claim should succeed");

    assert_eq!(
        claim.dispatch.status,
        project_store::NotificationDispatchStatus::Claimed
    );

    let duplicate = project_store::claim_notification_reply(
        &repo_root,
        &project_store::NotificationReplyClaimRequestRecord {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            route_id: second[0].route_id.clone(),
            correlation_key: second[0].correlation_key.clone(),
            responder_id: Some("operator-b".into()),
            reply_text: "late answer".into(),
            received_at: "2026-04-16T20:00:05Z".into(),
        },
    )
    .expect_err("duplicate claim should be rejected");
    assert_eq!(duplicate.code, "notification_reply_already_claimed");

    let approval = project_store::load_project_snapshot(&repo_root, project_id)
        .expect("load project snapshot after claim flow")
        .snapshot
        .approval_requests
        .into_iter()
        .find(|approval| approval.action_id == pending.action_id)
        .expect("pending approval should still exist");
    assert_eq!(approval.status, OperatorApprovalStatus::Pending);

    let connection = open_state_connection(&repo_root);
    let claim_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM notification_reply_claims WHERE project_id = ?1 AND action_id = ?2",
            params![project_id, pending.action_id.as_str()],
            |row| row.get(0),
        )
        .expect("count reply claim rows");
    assert_eq!(claim_count, 2);
}
