use std::path::{Path, PathBuf};

use cadence_desktop_lib::{
    db::{self, database_path_for_repo, project_store},
    git::repository::CanonicalRepository,
    state::DesktopState,
};
use rusqlite::{params, Connection};
use tempfile::TempDir;

fn seed_project(root: &TempDir, project_id: &str, repository_id: &str, repo_name: &str) -> PathBuf {
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
        status_entries: Vec::new(),
        has_staged_changes: false,
        has_unstaged_changes: false,
        has_untracked_changes: false,
    };

    let state = DesktopState::default();
    db::import_project(&repository, state.import_failpoints()).expect("import project");

    canonical_root
}

fn open_state_connection(repo_root: &Path) -> Connection {
    Connection::open(database_path_for_repo(repo_root)).expect("open repo-local database")
}

fn sample_run(project_id: &str, run_id: &str) -> project_store::RuntimeRunRecord {
    project_store::RuntimeRunRecord {
        project_id: project_id.into(),
        run_id: run_id.into(),
        runtime_kind: "openai_codex".into(),
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

fn sample_checkpoint(
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

fn sample_autonomous_run(project_id: &str, run_id: &str) -> project_store::AutonomousRunRecord {
    project_store::AutonomousRunRecord {
        project_id: project_id.into(),
        run_id: run_id.into(),
        runtime_kind: "openai_codex".into(),
        supervisor_kind: "detached_pty".into(),
        status: project_store::AutonomousRunStatus::Running,
        active_unit_sequence: Some(1),
        duplicate_start_detected: false,
        duplicate_start_run_id: None,
        duplicate_start_reason: None,
        started_at: "2099-04-15T19:00:00Z".into(),
        last_heartbeat_at: Some("2099-04-15T19:00:10Z".into()),
        last_checkpoint_at: Some("2099-04-15T19:00:20Z".into()),
        paused_at: None,
        cancelled_at: None,
        completed_at: None,
        crashed_at: None,
        stopped_at: None,
        pause_reason: None,
        cancel_reason: None,
        crash_reason: None,
        last_error: None,
        updated_at: "2099-04-15T19:00:20Z".into(),
    }
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

            CREATE TABLE IF NOT EXISTS operator_approvals (
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                action_id TEXT NOT NULL,
                session_id TEXT,
                flow_id TEXT,
                action_type TEXT NOT NULL,
                title TEXT NOT NULL,
                detail TEXT NOT NULL,
                status TEXT NOT NULL,
                decision_note TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                resolved_at TEXT,
                PRIMARY KEY (project_id, action_id),
                CHECK (action_id <> ''),
                CHECK (action_type <> ''),
                CHECK (title <> ''),
                CHECK (detail <> ''),
                CHECK (status IN ('pending', 'approved', 'rejected')),
                CHECK (
                    (status = 'pending' AND resolved_at IS NULL AND decision_note IS NULL)
                    OR (status IN ('approved', 'rejected') AND resolved_at IS NOT NULL)
                )
            );

            CREATE INDEX IF NOT EXISTS idx_operator_approvals_project_status_updated
                ON operator_approvals(project_id, status, updated_at DESC, created_at DESC);

            CREATE TABLE IF NOT EXISTS operator_verification_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                source_action_id TEXT,
                status TEXT NOT NULL,
                summary TEXT NOT NULL,
                detail TEXT,
                recorded_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                CHECK (status IN ('pending', 'passed', 'failed')),
                CHECK (summary <> ''),
                CHECK (source_action_id IS NULL OR source_action_id <> '')
            );

            CREATE INDEX IF NOT EXISTS idx_operator_verification_records_project_recorded
                ON operator_verification_records(project_id, recorded_at DESC, id DESC);

            CREATE TABLE IF NOT EXISTS operator_resume_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                source_action_id TEXT,
                session_id TEXT,
                status TEXT NOT NULL,
                summary TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                CHECK (status IN ('started', 'failed')),
                CHECK (summary <> ''),
                CHECK (source_action_id IS NULL OR source_action_id <> ''),
                CHECK (session_id IS NULL OR session_id <> '')
            );

            CREATE INDEX IF NOT EXISTS idx_operator_resume_history_project_created
                ON operator_resume_history(project_id, created_at DESC, id DESC);

            CREATE TABLE IF NOT EXISTS workflow_graph_nodes (
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                node_id TEXT NOT NULL,
                phase_id INTEGER NOT NULL CHECK (phase_id >= 0),
                sort_order INTEGER NOT NULL CHECK (sort_order >= 0),
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL,
                current_step TEXT,
                task_count INTEGER NOT NULL DEFAULT 0 CHECK (task_count >= 0),
                completed_tasks INTEGER NOT NULL DEFAULT 0 CHECK (completed_tasks >= 0),
                summary TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                PRIMARY KEY (project_id, node_id),
                UNIQUE (project_id, phase_id),
                UNIQUE (project_id, sort_order),
                CHECK (node_id <> ''),
                CHECK (status IN ('complete', 'active', 'pending', 'blocked')),
                CHECK (current_step IS NULL OR current_step IN ('discuss', 'plan', 'execute', 'verify', 'ship')),
                CHECK (completed_tasks <= task_count)
            );

            CREATE INDEX IF NOT EXISTS idx_workflow_graph_nodes_project_order
                ON workflow_graph_nodes(project_id, sort_order, phase_id);

            CREATE TABLE IF NOT EXISTS workflow_graph_edges (
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                from_node_id TEXT NOT NULL,
                to_node_id TEXT NOT NULL,
                transition_kind TEXT NOT NULL,
                gate_requirement TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                PRIMARY KEY (project_id, from_node_id, to_node_id),
                CHECK (from_node_id <> ''),
                CHECK (to_node_id <> ''),
                CHECK (transition_kind <> ''),
                CHECK (gate_requirement IS NULL OR gate_requirement <> ''),
                FOREIGN KEY (project_id, from_node_id)
                    REFERENCES workflow_graph_nodes(project_id, node_id) ON DELETE CASCADE,
                FOREIGN KEY (project_id, to_node_id)
                    REFERENCES workflow_graph_nodes(project_id, node_id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_workflow_graph_edges_project_from
                ON workflow_graph_edges(project_id, from_node_id, to_node_id);

            CREATE TABLE IF NOT EXISTS workflow_gate_metadata (
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                node_id TEXT NOT NULL,
                gate_key TEXT NOT NULL,
                gate_state TEXT NOT NULL,
                action_type TEXT,
                title TEXT,
                detail TEXT,
                decision_context TEXT,
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                PRIMARY KEY (project_id, node_id, gate_key),
                CHECK (gate_key <> ''),
                CHECK (gate_state IN ('pending', 'satisfied', 'blocked', 'skipped')),
                CHECK (action_type IS NULL OR action_type <> ''),
                CHECK (title IS NULL OR title <> ''),
                CHECK (detail IS NULL OR detail <> ''),
                CHECK (decision_context IS NULL OR decision_context <> ''),
                FOREIGN KEY (project_id, node_id)
                    REFERENCES workflow_graph_nodes(project_id, node_id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_workflow_gate_metadata_project_node
                ON workflow_gate_metadata(project_id, node_id, gate_state);

            CREATE TABLE IF NOT EXISTS workflow_transition_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                transition_id TEXT NOT NULL,
                causal_transition_id TEXT,
                from_node_id TEXT NOT NULL,
                to_node_id TEXT NOT NULL,
                transition_kind TEXT NOT NULL,
                gate_decision TEXT NOT NULL,
                gate_decision_context TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                CHECK (transition_id <> ''),
                CHECK (causal_transition_id IS NULL OR causal_transition_id <> ''),
                CHECK (from_node_id <> ''),
                CHECK (to_node_id <> ''),
                CHECK (transition_kind <> ''),
                CHECK (gate_decision IN ('approved', 'rejected', 'blocked', 'not_applicable')),
                CHECK (gate_decision_context IS NULL OR gate_decision_context <> ''),
                FOREIGN KEY (project_id, from_node_id)
                    REFERENCES workflow_graph_nodes(project_id, node_id) ON DELETE CASCADE,
                FOREIGN KEY (project_id, to_node_id)
                    REFERENCES workflow_graph_nodes(project_id, node_id) ON DELETE CASCADE,
                UNIQUE (project_id, transition_id)
            );

            CREATE INDEX IF NOT EXISTS idx_workflow_transition_events_project_created
                ON workflow_transition_events(project_id, created_at DESC, id DESC);
            CREATE INDEX IF NOT EXISTS idx_workflow_transition_events_project_nodes
                ON workflow_transition_events(project_id, from_node_id, to_node_id);
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
            VALUES (?1, 'legacy-repo', '', '', 0, 0, 0, 'main', 'openai_codex', '2026-04-13T18:00:00Z')
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

    connection
        .execute(
            r#"
            INSERT INTO runtime_sessions (
                project_id,
                runtime_kind,
                provider_id,
                flow_id,
                session_id,
                account_id,
                auth_phase,
                last_error_code,
                last_error_message,
                last_error_retryable,
                updated_at
            )
            VALUES (?1, 'openai_codex', 'openai_codex', NULL, 'session-auth', 'acct-1', 'authenticated', NULL, NULL, NULL, '2026-04-13T18:30:00Z')
            "#,
            [project_id],
        )
        .expect("insert legacy runtime session");

    database_path
}

#[test]
fn legacy_repo_local_state_is_upgraded_before_runtime_run_reads() {
    let root = tempfile::tempdir().expect("temp dir");
    let repo_root = root.path().join("legacy-repo");
    std::fs::create_dir_all(&repo_root).expect("create legacy repo root");
    let project_id = "project-legacy";
    let database_path = create_legacy_state_db(&repo_root, project_id);

    let recovered = project_store::load_runtime_run(&repo_root, project_id)
        .expect("load upgraded runtime run state");
    assert!(recovered.is_none());

    let connection = Connection::open(&database_path).expect("reopen upgraded database");
    let tables: Vec<String> = connection
        .prepare(
            r#"
            SELECT name
            FROM sqlite_master
            WHERE type = 'table'
              AND name IN ('runtime_runs', 'runtime_run_checkpoints')
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
            "runtime_run_checkpoints".to_string(),
            "runtime_runs".to_string(),
        ]
    );

    let auth_row: (String, String, String) = connection
        .query_row(
            "SELECT runtime_kind, provider_id, auth_phase FROM runtime_sessions WHERE project_id = ?1",
            [project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("load legacy auth row after migration");
    assert_eq!(
        auth_row,
        (
            "openai_codex".to_string(),
            "openai_codex".to_string(),
            "authenticated".to_string(),
        )
    );
}

#[test]
fn runtime_run_recovery_distinguishes_running_stale_stopped_and_failed_states() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");

    assert!(project_store::load_runtime_run(&repo_root, project_id)
        .expect("load empty runtime run state")
        .is_none());

    let run_id = "run-1";
    let running = sample_run(project_id, run_id);
    let first = project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: running.clone(),
            checkpoint: None,
        },
    )
    .expect("persist running runtime run without checkpoints");
    assert_eq!(first.run.status, project_store::RuntimeRunStatus::Running);
    assert!(first.checkpoints.is_empty());
    assert_eq!(first.last_checkpoint_sequence, 0);
    assert!(first.last_checkpoint_at.is_none());

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                updated_at: "2099-04-15T19:00:20Z".into(),
                last_heartbeat_at: Some("2099-04-15T19:00:20Z".into()),
                ..running.clone()
            },
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Supervisor launched and connected to the project PTY.",
                "2099-04-15T19:00:20Z",
            )),
        },
    )
    .expect("persist checkpoint one");

    let recovered = project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                updated_at: "2099-04-15T19:00:35Z".into(),
                last_heartbeat_at: Some("2099-04-15T19:00:35Z".into()),
                ..running.clone()
            },
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                2,
                project_store::RuntimeRunCheckpointKind::State,
                "Repository status collected; waiting for the next supervisor checkpoint.",
                "2099-04-15T19:00:35Z",
            )),
        },
    )
    .expect("persist checkpoint two");
    assert_eq!(
        recovered.run.status,
        project_store::RuntimeRunStatus::Running
    );
    assert_eq!(recovered.last_checkpoint_sequence, 2);
    assert_eq!(
        recovered.last_checkpoint_at.as_deref(),
        Some("2099-04-15T19:00:35Z")
    );
    assert_eq!(
        recovered
            .checkpoints
            .iter()
            .map(|checkpoint| checkpoint.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );

    let stale = project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                last_heartbeat_at: Some("2020-04-15T19:00:35Z".into()),
                updated_at: "2020-04-15T19:00:35Z".into(),
                ..running.clone()
            },
            checkpoint: None,
        },
    )
    .expect("persist stale runtime run");
    assert_eq!(stale.run.status, project_store::RuntimeRunStatus::Stale);

    let stopped = project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                status: project_store::RuntimeRunStatus::Stopped,
                stopped_at: Some("2099-04-15T19:01:10Z".into()),
                updated_at: "2099-04-15T19:01:10Z".into(),
                ..running.clone()
            },
            checkpoint: None,
        },
    )
    .expect("persist stopped runtime run");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
    assert_eq!(
        stopped.run.stopped_at.as_deref(),
        Some("2099-04-15T19:01:10Z")
    );

    let failed = project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                status: project_store::RuntimeRunStatus::Failed,
                last_error: Some(project_store::RuntimeRunDiagnosticRecord {
                    code: "supervisor_probe_failed".into(),
                    message: "The detached supervisor did not answer the control probe.".into(),
                }),
                updated_at: "2099-04-15T19:01:20Z".into(),
                ..running
            },
            checkpoint: None,
        },
    )
    .expect("persist failed runtime run");
    assert_eq!(failed.run.status, project_store::RuntimeRunStatus::Failed);
    assert_eq!(
        failed
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("supervisor_probe_failed")
    );
}

#[test]
fn runtime_run_checkpoint_writes_reject_secret_bearing_summaries_and_preserve_prior_sequence() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";
    let running = sample_run(project_id, run_id);

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: running.clone(),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Supervisor launched with a redacted startup summary.",
                "2099-04-15T19:00:20Z",
            )),
        },
    )
    .expect("persist safe checkpoint");

    let error = project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                updated_at: "2099-04-15T19:00:25Z".into(),
                last_heartbeat_at: Some("2099-04-15T19:00:25Z".into()),
                ..running
            },
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                2,
                project_store::RuntimeRunCheckpointKind::Diagnostic,
                "oauth redirect_uri=http://127.0.0.1:1455/auth/callback access_token=sk-live-secret",
                "2099-04-15T19:00:25Z",
            )),
        },
    )
    .expect_err("secret-bearing checkpoint summary should fail closed");
    assert_eq!(error.code, "runtime_run_checkpoint_invalid");

    let recovered = project_store::load_runtime_run(&repo_root, project_id)
        .expect("reload runtime run after rejected checkpoint")
        .expect("runtime run should still exist");
    assert_eq!(recovered.last_checkpoint_sequence, 1);
    assert_eq!(recovered.checkpoints.len(), 1);

    let database_bytes = std::fs::read(database_path_for_repo(&repo_root)).expect("read db bytes");
    let database_text = String::from_utf8_lossy(&database_bytes);
    assert!(!database_text.contains("sk-live-secret"));
    assert!(!database_text.contains("redirect_uri=http://127.0.0.1:1455/auth/callback"));
}

#[test]
fn runtime_run_decode_fails_closed_for_malformed_status_transport_and_checkpoint_kind() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Initial safe checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
        },
    )
    .expect("persist runtime run for corruption tests");

    let connection = open_state_connection(&repo_root);
    connection
        .execute_batch("PRAGMA ignore_check_constraints = 1;")
        .expect("disable check constraints");

    connection
        .execute(
            "UPDATE runtime_runs SET status = 'bogus_status' WHERE project_id = ?1",
            [project_id],
        )
        .expect("corrupt runtime run status");
    let error = project_store::load_runtime_run(&repo_root, project_id)
        .expect_err("malformed runtime run status should fail closed");
    assert_eq!(error.code, "runtime_run_decode_failed");

    connection
        .execute(
            "UPDATE runtime_runs SET status = 'running', transport_endpoint = '' WHERE project_id = ?1",
            [project_id],
        )
        .expect("corrupt transport metadata");
    let error = project_store::load_runtime_run(&repo_root, project_id)
        .expect_err("blank transport metadata should fail closed");
    assert_eq!(error.code, "runtime_run_decode_failed");

    connection
        .execute(
            "UPDATE runtime_runs SET transport_endpoint = '127.0.0.1:4455' WHERE project_id = ?1",
            [project_id],
        )
        .expect("repair transport metadata");
    connection
        .execute(
            "UPDATE runtime_run_checkpoints SET kind = 'bogus_kind' WHERE project_id = ?1 AND run_id = ?2 AND sequence = 1",
            params![project_id, run_id],
        )
        .expect("corrupt checkpoint kind");
    let error = project_store::load_runtime_run(&repo_root, project_id)
        .expect_err("malformed checkpoint kind should fail closed");
    assert_eq!(error.code, "runtime_run_checkpoint_decode_failed");
}

#[test]
fn runtime_run_checkpoint_sequence_must_increase_monotonically() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";
    let running = sample_run(project_id, run_id);

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: running.clone(),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "First checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
        },
    )
    .expect("persist first checkpoint");

    let error = project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                updated_at: "2099-04-15T19:00:25Z".into(),
                last_heartbeat_at: Some("2099-04-15T19:00:25Z".into()),
                ..running
            },
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::State,
                "Duplicate sequence should be rejected.",
                "2099-04-15T19:00:25Z",
            )),
        },
    )
    .expect_err("duplicate checkpoint sequence should fail closed");
    assert_eq!(error.code, "runtime_run_checkpoint_sequence_invalid");

    let recovered = project_store::load_runtime_run(&repo_root, project_id)
        .expect("reload runtime run after rejected sequence")
        .expect("runtime run should still exist");
    assert_eq!(recovered.last_checkpoint_sequence, 1);
    assert_eq!(recovered.checkpoints.len(), 1);
    assert_eq!(recovered.checkpoints[0].summary, "First checkpoint.");
}

#[test]
fn autonomous_run_persistence_tracks_current_unit_duplicate_start_and_cancel_metadata() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Supervisor launched and connected to the project PTY.",
                "2099-04-15T19:00:20Z",
            )),
        },
    )
    .expect("persist runtime run for autonomous projection");

    let persisted = project_store::upsert_autonomous_run(
        &repo_root,
        &sample_autonomous_run(project_id, run_id),
    )
    .expect("persist autonomous run");
    assert_eq!(persisted.run.status, project_store::AutonomousRunStatus::Running);
    assert_eq!(persisted.run.active_unit_sequence, Some(1));
    assert_eq!(
        persisted
            .unit_checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.summary.as_str()),
        Some("Supervisor launched and connected to the project PTY.")
    );

    let cancelled = project_store::upsert_autonomous_run(
        &repo_root,
        &project_store::AutonomousRunRecord {
            status: project_store::AutonomousRunStatus::Cancelled,
            duplicate_start_detected: true,
            duplicate_start_run_id: Some(run_id.into()),
            duplicate_start_reason: Some(
                "Cadence reused the already-active autonomous run for this project instead of launching a duplicate supervisor."
                    .into(),
            ),
            cancelled_at: Some("2099-04-15T19:01:05Z".into()),
            stopped_at: Some("2099-04-15T19:01:05Z".into()),
            cancel_reason: Some(project_store::RuntimeRunDiagnosticRecord {
                code: "autonomous_run_cancelled".into(),
                message: "Operator cancelled the autonomous run from the desktop shell.".into(),
            }),
            updated_at: "2099-04-15T19:01:05Z".into(),
            ..sample_autonomous_run(project_id, run_id)
        },
    )
    .expect("persist cancelled autonomous run");
    assert_eq!(
        cancelled.run.status,
        project_store::AutonomousRunStatus::Cancelled
    );
    assert!(cancelled.run.duplicate_start_detected);
    assert_eq!(cancelled.run.cancelled_at.as_deref(), Some("2099-04-15T19:01:05Z"));
    assert_eq!(
        cancelled
            .run
            .cancel_reason
            .as_ref()
            .map(|reason| reason.code.as_str()),
        Some("autonomous_run_cancelled")
    );

    let recovered = project_store::load_autonomous_run(&repo_root, project_id)
        .expect("reload autonomous run")
        .expect("autonomous run should still exist");
    assert_eq!(
        recovered.run.status,
        project_store::AutonomousRunStatus::Cancelled
    );
    assert!(recovered.run.duplicate_start_detected);
    assert_eq!(recovered.run.active_unit_sequence, Some(1));
    assert_eq!(
        recovered
            .unit_checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.sequence),
        Some(1)
    );
}

#[test]
fn autonomous_run_decode_fails_closed_when_unit_checkpoint_is_missing() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Bootstrap checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
        },
    )
    .expect("persist runtime run for autonomous decode failure");
    project_store::upsert_autonomous_run(
        &repo_root,
        &project_store::AutonomousRunRecord {
            active_unit_sequence: Some(1),
            ..sample_autonomous_run(project_id, run_id)
        },
    )
    .expect("persist autonomous run before corruption");

    let connection = open_state_connection(&repo_root);
    connection
        .execute(
            "DELETE FROM runtime_run_checkpoints WHERE project_id = ?1 AND run_id = ?2 AND sequence = 1",
            params![project_id, run_id],
        )
        .expect("delete active autonomous checkpoint");

    let error = project_store::load_autonomous_run(&repo_root, project_id)
        .expect_err("missing active-unit checkpoint should fail closed");
    assert_eq!(error.code, "runtime_run_decode_failed");
}
