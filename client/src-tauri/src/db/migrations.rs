use std::sync::LazyLock;

use rusqlite_migration::{Migrations, M};

pub fn migrations() -> &'static Migrations<'static> {
    static MIGRATIONS: LazyLock<Migrations<'static>> = LazyLock::new(|| {
        Migrations::new(vec![
            M::up(
                r#"
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
                "#,
            ),
            M::up(
                r#"
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
                "#,
            ),
            M::up(
                r#"
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
            ),
            M::up(
                r#"
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
                "#,
            ),
            M::up(
                r#"
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
                "#,
            ),
            M::up(
                r#"
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
            ),
            M::up(
                r#"
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
            ),
            M::up(
                r#"
                ALTER TABLE operator_approvals
                    ADD COLUMN gate_node_id TEXT CHECK (gate_node_id IS NULL OR gate_node_id <> '');
                ALTER TABLE operator_approvals
                    ADD COLUMN gate_key TEXT CHECK (gate_key IS NULL OR gate_key <> '');
                ALTER TABLE operator_approvals
                    ADD COLUMN transition_from_node_id TEXT CHECK (transition_from_node_id IS NULL OR transition_from_node_id <> '');
                ALTER TABLE operator_approvals
                    ADD COLUMN transition_to_node_id TEXT CHECK (transition_to_node_id IS NULL OR transition_to_node_id <> '');
                ALTER TABLE operator_approvals
                    ADD COLUMN transition_kind TEXT CHECK (transition_kind IS NULL OR transition_kind <> '');
                ALTER TABLE operator_approvals
                    ADD COLUMN user_answer TEXT CHECK (user_answer IS NULL OR user_answer <> '');

                CREATE INDEX IF NOT EXISTS idx_operator_approvals_project_gate_status_updated
                    ON operator_approvals(
                        project_id,
                        gate_node_id,
                        gate_key,
                        status,
                        updated_at DESC,
                        created_at DESC
                    );

                CREATE INDEX IF NOT EXISTS idx_operator_approvals_project_transition_target
                    ON operator_approvals(
                        project_id,
                        transition_from_node_id,
                        transition_to_node_id,
                        transition_kind,
                        status,
                        updated_at DESC
                    );
                "#,
            ),
            M::up(
                r#"
                CREATE TABLE IF NOT EXISTS runtime_runs (
                    project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
                    run_id TEXT NOT NULL UNIQUE,
                    runtime_kind TEXT NOT NULL,
                    supervisor_kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    transport_kind TEXT NOT NULL,
                    transport_endpoint TEXT NOT NULL,
                    transport_liveness TEXT NOT NULL DEFAULT 'unknown',
                    last_checkpoint_sequence INTEGER NOT NULL DEFAULT 0 CHECK (last_checkpoint_sequence >= 0),
                    started_at TEXT NOT NULL,
                    last_heartbeat_at TEXT,
                    last_checkpoint_at TEXT,
                    stopped_at TEXT,
                    last_error_code TEXT,
                    last_error_message TEXT,
                    updated_at TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    CHECK (run_id <> ''),
                    CHECK (runtime_kind <> ''),
                    CHECK (supervisor_kind <> ''),
                    CHECK (status IN ('starting', 'running', 'stale', 'stopped', 'failed')),
                    CHECK (transport_kind <> ''),
                    CHECK (transport_endpoint <> ''),
                    CHECK (transport_liveness IN ('unknown', 'reachable', 'unreachable')),
                    CHECK (
                        (last_error_code IS NULL AND last_error_message IS NULL)
                        OR (last_error_code IS NOT NULL AND last_error_message IS NOT NULL)
                    )
                );

                CREATE UNIQUE INDEX IF NOT EXISTS idx_runtime_runs_project_run
                    ON runtime_runs(project_id, run_id);
                CREATE INDEX IF NOT EXISTS idx_runtime_runs_status_updated
                    ON runtime_runs(project_id, status, updated_at DESC);
                CREATE INDEX IF NOT EXISTS idx_runtime_runs_supervisor_kind
                    ON runtime_runs(supervisor_kind, transport_liveness);

                CREATE TABLE IF NOT EXISTS runtime_run_checkpoints (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    sequence INTEGER NOT NULL CHECK (sequence > 0),
                    kind TEXT NOT NULL,
                    summary TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    CHECK (kind IN ('bootstrap', 'state', 'tool', 'action_required', 'diagnostic')),
                    CHECK (summary <> ''),
                    UNIQUE (project_id, run_id, sequence),
                    FOREIGN KEY (project_id, run_id)
                        REFERENCES runtime_runs(project_id, run_id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_runtime_run_checkpoints_project_run_sequence
                    ON runtime_run_checkpoints(project_id, run_id, sequence DESC);
                CREATE INDEX IF NOT EXISTS idx_runtime_run_checkpoints_project_created
                    ON runtime_run_checkpoints(project_id, created_at DESC, id DESC);
                "#,
            ),
            M::up(
                r#"
                CREATE TABLE IF NOT EXISTS workflow_handoff_packages (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                    handoff_transition_id TEXT NOT NULL,
                    causal_transition_id TEXT,
                    from_node_id TEXT NOT NULL,
                    to_node_id TEXT NOT NULL,
                    transition_kind TEXT NOT NULL,
                    package_payload TEXT NOT NULL,
                    package_hash TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    CHECK (handoff_transition_id <> ''),
                    CHECK (causal_transition_id IS NULL OR causal_transition_id <> ''),
                    CHECK (from_node_id <> ''),
                    CHECK (to_node_id <> ''),
                    CHECK (transition_kind <> ''),
                    CHECK (package_payload <> ''),
                    CHECK (json_valid(package_payload)),
                    CHECK (length(package_hash) = 64),
                    CHECK (package_hash NOT GLOB '*[^0-9a-f]*'),
                    FOREIGN KEY (project_id, handoff_transition_id)
                        REFERENCES workflow_transition_events(project_id, transition_id) ON DELETE CASCADE,
                    FOREIGN KEY (project_id, from_node_id)
                        REFERENCES workflow_graph_nodes(project_id, node_id) ON DELETE CASCADE,
                    FOREIGN KEY (project_id, to_node_id)
                        REFERENCES workflow_graph_nodes(project_id, node_id) ON DELETE CASCADE,
                    UNIQUE (project_id, handoff_transition_id)
                );

                CREATE INDEX IF NOT EXISTS idx_workflow_handoff_packages_project_created
                    ON workflow_handoff_packages(project_id, created_at DESC, id DESC);
                CREATE INDEX IF NOT EXISTS idx_workflow_handoff_packages_project_causal
                    ON workflow_handoff_packages(project_id, causal_transition_id, created_at DESC, id DESC);
                "#,
            ),
            M::up(
                r#"
                CREATE TABLE IF NOT EXISTS notification_routes (
                    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                    route_id TEXT NOT NULL,
                    route_kind TEXT NOT NULL,
                    route_target TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
                    metadata_json TEXT,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    PRIMARY KEY (project_id, route_id),
                    CHECK (route_id <> ''),
                    CHECK (route_kind <> ''),
                    CHECK (route_target <> ''),
                    CHECK (metadata_json IS NULL OR (metadata_json <> '' AND json_valid(metadata_json)))
                );

                CREATE INDEX IF NOT EXISTS idx_notification_routes_project_enabled
                    ON notification_routes(project_id, enabled, updated_at DESC, route_id ASC);

                CREATE TABLE IF NOT EXISTS notification_dispatches (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    project_id TEXT NOT NULL,
                    action_id TEXT NOT NULL,
                    route_id TEXT NOT NULL,
                    correlation_key TEXT NOT NULL,
                    status TEXT NOT NULL,
                    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
                    last_attempt_at TEXT,
                    delivered_at TEXT,
                    claimed_at TEXT,
                    last_error_code TEXT,
                    last_error_message TEXT,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    CHECK (action_id <> ''),
                    CHECK (route_id <> ''),
                    CHECK (correlation_key <> ''),
                    CHECK (status IN ('pending', 'sent', 'failed', 'claimed')),
                    CHECK (last_error_code IS NULL OR last_error_code <> ''),
                    CHECK (last_error_message IS NULL OR last_error_message <> ''),
                    UNIQUE (project_id, action_id, route_id),
                    UNIQUE (project_id, correlation_key),
                    FOREIGN KEY (project_id, action_id)
                        REFERENCES operator_approvals(project_id, action_id) ON DELETE CASCADE,
                    FOREIGN KEY (project_id, route_id)
                        REFERENCES notification_routes(project_id, route_id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_notification_dispatches_project_status_updated
                    ON notification_dispatches(project_id, status, updated_at DESC, id DESC);
                CREATE INDEX IF NOT EXISTS idx_notification_dispatches_project_action
                    ON notification_dispatches(project_id, action_id, route_id, id DESC);

                CREATE TABLE IF NOT EXISTS notification_reply_claims (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                    action_id TEXT NOT NULL,
                    route_id TEXT NOT NULL,
                    correlation_key TEXT NOT NULL,
                    responder_id TEXT,
                    reply_text TEXT NOT NULL,
                    status TEXT NOT NULL,
                    rejection_code TEXT,
                    rejection_message TEXT,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    CHECK (action_id <> ''),
                    CHECK (route_id <> ''),
                    CHECK (correlation_key <> ''),
                    CHECK (reply_text <> ''),
                    CHECK (responder_id IS NULL OR responder_id <> ''),
                    CHECK (status IN ('accepted', 'rejected')),
                    CHECK (rejection_code IS NULL OR rejection_code <> ''),
                    CHECK (rejection_message IS NULL OR rejection_message <> ''),
                    CHECK (
                        (status = 'accepted' AND rejection_code IS NULL AND rejection_message IS NULL)
                        OR (status = 'rejected' AND rejection_code IS NOT NULL AND rejection_message IS NOT NULL)
                    )
                );

                CREATE UNIQUE INDEX IF NOT EXISTS idx_notification_reply_claims_project_action_winner
                    ON notification_reply_claims(project_id, action_id)
                    WHERE status = 'accepted';

                CREATE INDEX IF NOT EXISTS idx_notification_reply_claims_project_action_created
                    ON notification_reply_claims(project_id, action_id, created_at DESC, id DESC);
                CREATE INDEX IF NOT EXISTS idx_notification_reply_claims_project_route_created
                    ON notification_reply_claims(project_id, route_id, created_at DESC, id DESC);
                "#,
            ),
        ])
    });

    &MIGRATIONS
}
