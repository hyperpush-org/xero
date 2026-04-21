pub(crate) use std::path::{Path, PathBuf};

pub(crate) use cadence_desktop_lib::{
    db::{self, database_path_for_repo, project_store},
    git::repository::CanonicalRepository,
    runtime::{
        autonomous_orchestrator::{persist_skill_lifecycle_event, AutonomousSkillLifecycleEvent},
        AutonomousSkillCacheStatus, AutonomousSkillSourceMetadata,
    },
    state::DesktopState,
};
pub(crate) use rusqlite::{params, Connection};
pub(crate) use sha2::{Digest, Sha256};
pub(crate) use tempfile::TempDir;

pub(crate) fn seed_project(root: &TempDir, project_id: &str, repository_id: &str, repo_name: &str) -> PathBuf {
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

pub(crate) fn open_state_connection(repo_root: &Path) -> Connection {
    Connection::open(database_path_for_repo(repo_root)).expect("open repo-local database")
}

pub(crate) fn sample_run(project_id: &str, run_id: &str) -> project_store::RuntimeRunRecord {
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

pub(crate) fn sample_autonomous_run_record(
    project_id: &str,
    run_id: &str,
) -> project_store::AutonomousRunRecord {
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

pub(crate) fn sample_autonomous_workflow_linkage() -> project_store::AutonomousWorkflowLinkageRecord {
    project_store::AutonomousWorkflowLinkageRecord {
        workflow_node_id: "workflow-research".into(),
        transition_id: "auto:txn-002:workflow-discussion:workflow-research".into(),
        causal_transition_id: Some("txn-001".into()),
        handoff_transition_id: "auto:txn-002:workflow-discussion:workflow-research".into(),
        handoff_package_hash: "f2a21cec422a39086c026fa96b38f2875b83faabc49461e979c5504c34b2640e"
            .into(),
    }
}

pub(crate) fn sample_autonomous_unit(project_id: &str, run_id: &str) -> project_store::AutonomousUnitRecord {
    project_store::AutonomousUnitRecord {
        project_id: project_id.into(),
        run_id: run_id.into(),
        unit_id: format!("{run_id}:unit:1"),
        sequence: 1,
        kind: project_store::AutonomousUnitKind::Researcher,
        status: project_store::AutonomousUnitStatus::Active,
        summary: "Researcher child session launched.".into(),
        boundary_id: None,
        workflow_linkage: None,
        started_at: "2099-04-15T19:00:00Z".into(),
        finished_at: None,
        updated_at: "2099-04-15T19:00:20Z".into(),
        last_error: None,
    }
}

pub(crate) fn sample_autonomous_attempt(
    project_id: &str,
    run_id: &str,
) -> project_store::AutonomousUnitAttemptRecord {
    project_store::AutonomousUnitAttemptRecord {
        project_id: project_id.into(),
        run_id: run_id.into(),
        unit_id: format!("{run_id}:unit:1"),
        attempt_id: format!("{run_id}:unit:1:attempt:1"),
        attempt_number: 1,
        child_session_id: "child-session-1".into(),
        status: project_store::AutonomousUnitStatus::Active,
        boundary_id: None,
        workflow_linkage: None,
        started_at: "2099-04-15T19:00:00Z".into(),
        finished_at: None,
        updated_at: "2099-04-15T19:00:20Z".into(),
        last_error: None,
    }
}

pub(crate) fn sample_tool_result_artifact(
    project_id: &str,
    run_id: &str,
) -> project_store::AutonomousUnitArtifactRecord {
    let unit_id = format!("{run_id}:unit:1");
    let attempt_id = format!("{run_id}:unit:1:attempt:1");
    let artifact_id = "artifact-tool-result".to_string();

    project_store::AutonomousUnitArtifactRecord {
        project_id: project_id.into(),
        run_id: run_id.into(),
        unit_id: unit_id.clone(),
        attempt_id: attempt_id.clone(),
        artifact_id: artifact_id.clone(),
        artifact_kind: "tool_result".into(),
        status: project_store::AutonomousUnitArtifactStatus::Recorded,
        summary: "Shell tool result persisted for the active executor attempt.".into(),
        content_hash: None,
        payload: Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(
            project_store::AutonomousToolResultPayloadRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                unit_id,
                attempt_id,
                artifact_id,
                tool_call_id: "tool-call-1".into(),
                tool_name: "shell.exec".into(),
                tool_state: project_store::AutonomousToolCallStateRecord::Succeeded,
                command_result: Some(project_store::AutonomousArtifactCommandResultRecord {
                    exit_code: Some(0),
                    timed_out: false,
                    summary: "Command exited successfully after capturing structured evidence."
                        .into(),
                }),
                tool_summary: Some(
                    cadence_desktop_lib::runtime::protocol::ToolResultSummary::Command(
                        cadence_desktop_lib::runtime::protocol::CommandToolResultSummary {
                            exit_code: Some(0),
                            timed_out: false,
                            stdout_truncated: false,
                            stderr_truncated: false,
                            stdout_redacted: false,
                            stderr_redacted: false,
                        },
                    ),
                ),
                action_id: Some("action-1".into()),
                boundary_id: Some("boundary-1".into()),
            },
        )),
        created_at: "2099-04-15T19:00:20Z".into(),
        updated_at: "2099-04-15T19:00:20Z".into(),
    }
}

pub(crate) fn sample_verification_evidence_artifact(
    project_id: &str,
    run_id: &str,
) -> project_store::AutonomousUnitArtifactRecord {
    let unit_id = format!("{run_id}:unit:1");
    let attempt_id = format!("{run_id}:unit:1:attempt:1");
    let artifact_id = "artifact-verification-evidence".to_string();

    project_store::AutonomousUnitArtifactRecord {
        project_id: project_id.into(),
        run_id: run_id.into(),
        unit_id: unit_id.clone(),
        attempt_id: attempt_id.clone(),
        artifact_id: artifact_id.clone(),
        artifact_kind: "verification_evidence".into(),
        status: project_store::AutonomousUnitArtifactStatus::Recorded,
        summary: "Autonomous attempt paused on terminal input and recorded deterministic boundary evidence.".into(),
        content_hash: None,
        payload: Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(
            project_store::AutonomousVerificationEvidencePayloadRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                unit_id,
                attempt_id,
                artifact_id,
                evidence_kind: "terminal_input_required".into(),
                label: "Terminal input required".into(),
                outcome: project_store::AutonomousVerificationOutcomeRecord::Blocked,
                command_result: None,
                action_id: Some("action-1".into()),
                boundary_id: Some("boundary-1".into()),
            },
        )),
        created_at: "2099-04-15T19:00:20Z".into(),
        updated_at: "2099-04-15T19:00:20Z".into(),
    }
}

pub(crate) fn sample_skill_source_metadata() -> AutonomousSkillSourceMetadata {
    AutonomousSkillSourceMetadata {
        repo: "vercel-labs/skills".into(),
        path: "skills/find-skills".into(),
        reference: "main".into(),
        tree_hash: "0123456789abcdef0123456789abcdef01234567".into(),
    }
}

pub(crate) fn sample_skill_lifecycle_artifact(
    project_id: &str,
    run_id: &str,
) -> project_store::AutonomousUnitArtifactRecord {
    let unit_id = format!("{run_id}:unit:1");
    let attempt_id = format!("{run_id}:unit:1:attempt:1");
    let artifact_id = "artifact-skill-lifecycle-discovery".to_string();

    project_store::AutonomousUnitArtifactRecord {
        project_id: project_id.into(),
        run_id: run_id.into(),
        unit_id: unit_id.clone(),
        attempt_id: attempt_id.clone(),
        artifact_id: artifact_id.clone(),
        artifact_kind: "skill_lifecycle".into(),
        status: project_store::AutonomousUnitArtifactStatus::Recorded,
        summary: "Autonomous skill `find-skills` recorded a successful discovery stage.".into(),
        content_hash: None,
        payload: Some(
            project_store::AutonomousArtifactPayloadRecord::SkillLifecycle(
                project_store::AutonomousSkillLifecyclePayloadRecord {
                    project_id: project_id.into(),
                    run_id: run_id.into(),
                    unit_id,
                    attempt_id,
                    artifact_id,
                    stage: project_store::AutonomousSkillLifecycleStageRecord::Discovery,
                    result: project_store::AutonomousSkillLifecycleResultRecord::Succeeded,
                    skill_id: "find-skills".into(),
                    source: project_store::AutonomousSkillLifecycleSourceRecord {
                        repo: "vercel-labs/skills".into(),
                        path: "skills/find-skills".into(),
                        reference: "main".into(),
                        tree_hash: "0123456789abcdef0123456789abcdef01234567".into(),
                    },
                    cache: project_store::AutonomousSkillLifecycleCacheRecord {
                        key: "find-skills-576b45048241".into(),
                        status: None,
                    },
                    diagnostic: None,
                },
            ),
        ),
        created_at: "2099-04-15T19:00:20Z".into(),
        updated_at: "2099-04-15T19:00:20Z".into(),
    }
}

pub(crate) fn sample_autonomous_run(
    project_id: &str,
    run_id: &str,
) -> project_store::AutonomousRunUpsertRecord {
    project_store::AutonomousRunUpsertRecord {
        run: sample_autonomous_run_record(project_id, run_id),
        unit: Some(sample_autonomous_unit(project_id, run_id)),
        attempt: Some(sample_autonomous_attempt(project_id, run_id)),
        artifacts: Vec::new(),
    }
}

pub(crate) fn seed_autonomous_workflow_linkage_rows(repo_root: &Path, project_id: &str) {
    let connection = open_state_connection(repo_root);
    let handoff_transition_id = "auto:txn-002:workflow-discussion:workflow-research";
    let package_payload = "{\"schemaVersion\":1,\"triggerTransition\":{\"transitionId\":\"auto:txn-002:workflow-discussion:workflow-research\"}}";

    connection
        .execute(
            r#"
            INSERT OR IGNORE INTO workflow_graph_nodes (
                project_id,
                node_id,
                phase_id,
                sort_order,
                name,
                description,
                status,
                summary,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, '', ?6, NULL, '2099-04-15T19:00:00Z')
            "#,
            params![
                project_id,
                "workflow-discussion",
                1_i64,
                1_i64,
                "Discussion",
                "complete"
            ],
        )
        .expect("insert workflow discussion node");
    connection
        .execute(
            r#"
            INSERT OR IGNORE INTO workflow_graph_nodes (
                project_id,
                node_id,
                phase_id,
                sort_order,
                name,
                description,
                status,
                summary,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, '', ?6, NULL, '2099-04-15T19:00:00Z')
            "#,
            params![
                project_id,
                "workflow-research",
                2_i64,
                2_i64,
                "Research",
                "active"
            ],
        )
        .expect("insert workflow research node");
    connection
        .execute(
            r#"
            INSERT OR IGNORE INTO workflow_transition_events (
                project_id,
                transition_id,
                causal_transition_id,
                from_node_id,
                to_node_id,
                transition_kind,
                gate_decision,
                gate_decision_context,
                created_at
            )
            VALUES (?1, ?2, ?3, 'workflow-discussion', 'workflow-research', 'advance', 'approved', 'operator-approved', '2099-04-15T19:00:00Z')
            "#,
            params![project_id, handoff_transition_id, "txn-001"],
        )
        .expect("insert workflow transition event");
    connection
        .execute(
            r#"
            INSERT OR IGNORE INTO workflow_handoff_packages (
                project_id,
                handoff_transition_id,
                causal_transition_id,
                from_node_id,
                to_node_id,
                transition_kind,
                package_payload,
                package_hash,
                created_at
            )
            VALUES (?1, ?2, ?3, 'workflow-discussion', 'workflow-research', 'advance', ?4, ?5, '2099-04-15T19:00:01Z')
            "#,
            params![
                project_id,
                handoff_transition_id,
                "txn-001",
                package_payload,
                sample_autonomous_workflow_linkage().handoff_package_hash,
            ],
        )
        .expect("insert workflow handoff package");
}

pub(crate) fn create_legacy_state_db(repo_root: &Path, project_id: &str) -> PathBuf {
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
