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
