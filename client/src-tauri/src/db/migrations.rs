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
                    CHECK (current_step IS NULL OR current_step <> ''),
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
            M::up(
                r#"
                CREATE TABLE IF NOT EXISTS autonomous_runs (
                    project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
                    run_id TEXT NOT NULL,
                    runtime_kind TEXT NOT NULL,
                    supervisor_kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    active_unit_sequence INTEGER,
                    duplicate_start_detected INTEGER NOT NULL DEFAULT 0 CHECK (duplicate_start_detected IN (0, 1)),
                    duplicate_start_run_id TEXT,
                    duplicate_start_reason TEXT,
                    started_at TEXT NOT NULL,
                    last_heartbeat_at TEXT,
                    last_checkpoint_at TEXT,
                    paused_at TEXT,
                    cancelled_at TEXT,
                    completed_at TEXT,
                    crashed_at TEXT,
                    stopped_at TEXT,
                    pause_reason_code TEXT,
                    pause_reason_message TEXT,
                    cancel_reason_code TEXT,
                    cancel_reason_message TEXT,
                    crash_reason_code TEXT,
                    crash_reason_message TEXT,
                    last_error_code TEXT,
                    last_error_message TEXT,
                    updated_at TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    CHECK (run_id <> ''),
                    CHECK (runtime_kind <> ''),
                    CHECK (supervisor_kind <> ''),
                    CHECK (status IN ('starting', 'running', 'paused', 'cancelling', 'cancelled', 'stale', 'failed', 'stopped', 'crashed', 'completed')),
                    CHECK (active_unit_sequence IS NULL OR active_unit_sequence > 0),
                    CHECK (duplicate_start_run_id IS NULL OR duplicate_start_run_id <> ''),
                    CHECK (duplicate_start_reason IS NULL OR duplicate_start_reason <> ''),
                    CHECK (
                        (pause_reason_code IS NULL AND pause_reason_message IS NULL)
                        OR (pause_reason_code IS NOT NULL AND pause_reason_message IS NOT NULL)
                    ),
                    CHECK (
                        (cancel_reason_code IS NULL AND cancel_reason_message IS NULL)
                        OR (cancel_reason_code IS NOT NULL AND cancel_reason_message IS NOT NULL)
                    ),
                    CHECK (
                        (crash_reason_code IS NULL AND crash_reason_message IS NULL)
                        OR (crash_reason_code IS NOT NULL AND crash_reason_message IS NOT NULL)
                    ),
                    CHECK (
                        (last_error_code IS NULL AND last_error_message IS NULL)
                        OR (last_error_code IS NOT NULL AND last_error_message IS NOT NULL)
                    ),
                    FOREIGN KEY (project_id, run_id)
                        REFERENCES runtime_runs(project_id, run_id) ON DELETE CASCADE
                );

                CREATE UNIQUE INDEX IF NOT EXISTS idx_autonomous_runs_project_run
                    ON autonomous_runs(project_id, run_id);
                CREATE INDEX IF NOT EXISTS idx_autonomous_runs_status_updated
                    ON autonomous_runs(project_id, status, updated_at DESC);
                "#,
            ),
            M::up(
                r#"
                CREATE TABLE IF NOT EXISTS autonomous_units (
                    unit_id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    sequence INTEGER NOT NULL CHECK (sequence > 0),
                    kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    summary TEXT NOT NULL,
                    boundary_id TEXT,
                    started_at TEXT NOT NULL,
                    finished_at TEXT,
                    last_error_code TEXT,
                    last_error_message TEXT,
                    updated_at TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    CHECK (unit_id <> ''),
                    CHECK (kind <> ''),
                    CHECK (status IN ('pending', 'active', 'blocked', 'paused', 'completed', 'cancelled', 'failed')),
                    CHECK (summary <> ''),
                    CHECK (boundary_id IS NULL OR boundary_id <> ''),
                    CHECK (
                        (last_error_code IS NULL AND last_error_message IS NULL)
                        OR (last_error_code IS NOT NULL AND last_error_message IS NOT NULL)
                    ),
                    UNIQUE (project_id, run_id, sequence),
                    UNIQUE (project_id, run_id, unit_id),
                    FOREIGN KEY (project_id, run_id)
                        REFERENCES autonomous_runs(project_id, run_id) ON DELETE CASCADE
                );

                CREATE UNIQUE INDEX IF NOT EXISTS idx_autonomous_units_one_active_per_run
                    ON autonomous_units(project_id, run_id)
                    WHERE status = 'active';
                CREATE INDEX IF NOT EXISTS idx_autonomous_units_project_run_sequence
                    ON autonomous_units(project_id, run_id, sequence DESC, updated_at DESC);

                CREATE TABLE IF NOT EXISTS autonomous_unit_attempts (
                    attempt_id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    unit_id TEXT NOT NULL,
                    attempt_number INTEGER NOT NULL CHECK (attempt_number > 0),
                    child_session_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    boundary_id TEXT,
                    started_at TEXT NOT NULL,
                    finished_at TEXT,
                    last_error_code TEXT,
                    last_error_message TEXT,
                    updated_at TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    CHECK (attempt_id <> ''),
                    CHECK (child_session_id <> ''),
                    CHECK (status IN ('pending', 'active', 'blocked', 'paused', 'completed', 'cancelled', 'failed')),
                    CHECK (boundary_id IS NULL OR boundary_id <> ''),
                    CHECK (
                        (last_error_code IS NULL AND last_error_message IS NULL)
                        OR (last_error_code IS NOT NULL AND last_error_message IS NOT NULL)
                    ),
                    UNIQUE (project_id, run_id, unit_id, attempt_number),
                    UNIQUE (project_id, run_id, child_session_id),
                    UNIQUE (project_id, run_id, attempt_id),
                    FOREIGN KEY (project_id, run_id, unit_id)
                        REFERENCES autonomous_units(project_id, run_id, unit_id) ON DELETE CASCADE
                );

                CREATE UNIQUE INDEX IF NOT EXISTS idx_autonomous_unit_attempts_one_active_per_run
                    ON autonomous_unit_attempts(project_id, run_id)
                    WHERE status = 'active';
                CREATE INDEX IF NOT EXISTS idx_autonomous_unit_attempts_project_run_unit_attempt
                    ON autonomous_unit_attempts(project_id, run_id, unit_id, attempt_number DESC, updated_at DESC);

                CREATE TABLE IF NOT EXISTS autonomous_unit_artifacts (
                    artifact_id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    unit_id TEXT NOT NULL,
                    attempt_id TEXT NOT NULL,
                    artifact_kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    summary TEXT NOT NULL,
                    content_hash TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    CHECK (artifact_id <> ''),
                    CHECK (artifact_kind <> ''),
                    CHECK (status IN ('pending', 'recorded', 'rejected', 'redacted')),
                    CHECK (summary <> ''),
                    CHECK (content_hash IS NULL OR (length(content_hash) = 64 AND content_hash NOT GLOB '*[^0-9a-f]*')),
                    UNIQUE (project_id, run_id, artifact_id),
                    FOREIGN KEY (project_id, run_id, unit_id)
                        REFERENCES autonomous_units(project_id, run_id, unit_id) ON DELETE CASCADE,
                    FOREIGN KEY (project_id, run_id, attempt_id)
                        REFERENCES autonomous_unit_attempts(project_id, run_id, attempt_id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_autonomous_unit_artifacts_project_run_attempt
                    ON autonomous_unit_artifacts(project_id, run_id, attempt_id, created_at DESC, artifact_id ASC);
                "#,
            ),
            M::up(
                r#"
                ALTER TABLE autonomous_unit_artifacts
                    ADD COLUMN payload_json TEXT;
                "#,
            ),
            M::up(
                r#"
                ALTER TABLE autonomous_units
                    ADD COLUMN workflow_node_id TEXT;
                ALTER TABLE autonomous_units
                    ADD COLUMN workflow_transition_id TEXT;
                ALTER TABLE autonomous_units
                    ADD COLUMN workflow_causal_transition_id TEXT;
                ALTER TABLE autonomous_units
                    ADD COLUMN workflow_handoff_transition_id TEXT;
                ALTER TABLE autonomous_units
                    ADD COLUMN workflow_handoff_package_hash TEXT;

                ALTER TABLE autonomous_unit_attempts
                    ADD COLUMN workflow_node_id TEXT;
                ALTER TABLE autonomous_unit_attempts
                    ADD COLUMN workflow_transition_id TEXT;
                ALTER TABLE autonomous_unit_attempts
                    ADD COLUMN workflow_causal_transition_id TEXT;
                ALTER TABLE autonomous_unit_attempts
                    ADD COLUMN workflow_handoff_transition_id TEXT;
                ALTER TABLE autonomous_unit_attempts
                    ADD COLUMN workflow_handoff_package_hash TEXT;
                "#,
            ),
            M::up(
                r#"
                ALTER TABLE runtime_runs
                    ADD COLUMN control_state_json TEXT;

                UPDATE runtime_runs
                SET control_state_json = json_object(
                    'active',
                    json_object(
                        'modelId', runtime_kind,
                        'thinkingEffort', NULL,
                        'approvalMode', 'suggest',
                        'revision', 1,
                        'appliedAt', COALESCE(started_at, updated_at, created_at)
                    ),
                    'pending', NULL
                )
                WHERE control_state_json IS NULL;
                "#,
            ),
            M::up(
                r#"
                ALTER TABLE runtime_runs
                    ADD COLUMN provider_id TEXT;

                UPDATE runtime_runs
                SET provider_id = CASE runtime_kind
                    WHEN 'openai_codex' THEN 'openai_codex'
                    WHEN 'openrouter' THEN 'openrouter'
                    WHEN 'anthropic' THEN 'anthropic'
                    ELSE NULL
                END
                WHERE provider_id IS NULL;

                CREATE INDEX IF NOT EXISTS idx_runtime_runs_provider_status_updated
                    ON runtime_runs(provider_id, status, updated_at DESC);

                ALTER TABLE autonomous_runs
                    ADD COLUMN provider_id TEXT;

                UPDATE autonomous_runs
                SET provider_id = COALESCE(
                    (
                        SELECT runtime_runs.provider_id
                        FROM runtime_runs
                        WHERE runtime_runs.project_id = autonomous_runs.project_id
                          AND runtime_runs.run_id = autonomous_runs.run_id
                    ),
                    CASE runtime_kind
                        WHEN 'openai_codex' THEN 'openai_codex'
                        WHEN 'openrouter' THEN 'openrouter'
                        WHEN 'anthropic' THEN 'anthropic'
                        ELSE NULL
                    END
                )
                WHERE provider_id IS NULL;

                CREATE INDEX IF NOT EXISTS idx_autonomous_runs_provider_status_updated
                    ON autonomous_runs(provider_id, status, updated_at DESC);
                "#,
            ),
            M::up(
                r#"
                CREATE TABLE IF NOT EXISTS agent_sessions (
                    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                    agent_session_id TEXT NOT NULL,
                    title TEXT NOT NULL,
                    summary TEXT NOT NULL DEFAULT '',
                    status TEXT NOT NULL,
                    selected INTEGER NOT NULL DEFAULT 0 CHECK (selected IN (0, 1)),
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    archived_at TEXT,
                    last_run_id TEXT,
                    last_runtime_kind TEXT,
                    last_provider_id TEXT,
                    PRIMARY KEY (project_id, agent_session_id),
                    CHECK (agent_session_id <> ''),
                    CHECK (title <> ''),
                    CHECK (status IN ('active', 'archived')),
                    CHECK (last_run_id IS NULL OR last_run_id <> ''),
                    CHECK (last_runtime_kind IS NULL OR last_runtime_kind <> ''),
                    CHECK (last_provider_id IS NULL OR last_provider_id <> ''),
                    CHECK (
                        (status = 'active' AND archived_at IS NULL)
                        OR (status = 'archived' AND archived_at IS NOT NULL)
                    )
                );

                INSERT OR IGNORE INTO agent_sessions (
                    project_id,
                    agent_session_id,
                    title,
                    summary,
                    status,
                    selected,
                    created_at,
                    updated_at,
                    last_run_id,
                    last_runtime_kind,
                    last_provider_id
                )
                SELECT
                    projects.id,
                    'agent-session-main',
                    'Main',
                    '',
                    'active',
                    1,
                    COALESCE(projects.created_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                    runtime_runs.run_id,
                    runtime_runs.runtime_kind,
                    runtime_runs.provider_id
                FROM projects
                LEFT JOIN runtime_runs
                    ON runtime_runs.project_id = projects.id;

                CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_sessions_selected
                    ON agent_sessions(project_id)
                    WHERE selected = 1;
                CREATE INDEX IF NOT EXISTS idx_agent_sessions_project_status_updated
                    ON agent_sessions(project_id, status, updated_at DESC);
                CREATE INDEX IF NOT EXISTS idx_agent_sessions_project_last_run
                    ON agent_sessions(project_id, last_run_id)
                    WHERE last_run_id IS NOT NULL;

                CREATE TABLE runtime_runs_session_scoped (
                    project_id TEXT NOT NULL,
                    agent_session_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    runtime_kind TEXT NOT NULL,
                    provider_id TEXT NOT NULL,
                    supervisor_kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    transport_kind TEXT NOT NULL,
                    transport_endpoint TEXT NOT NULL,
                    transport_liveness TEXT NOT NULL DEFAULT 'unknown',
                    control_state_json TEXT NOT NULL,
                    last_checkpoint_sequence INTEGER NOT NULL DEFAULT 0 CHECK (last_checkpoint_sequence >= 0),
                    started_at TEXT NOT NULL,
                    last_heartbeat_at TEXT,
                    last_checkpoint_at TEXT,
                    stopped_at TEXT,
                    last_error_code TEXT,
                    last_error_message TEXT,
                    updated_at TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    PRIMARY KEY (project_id, agent_session_id),
                    UNIQUE (project_id, run_id),
                    CHECK (agent_session_id <> ''),
                    CHECK (run_id <> ''),
                    CHECK (runtime_kind <> ''),
                    CHECK (provider_id <> ''),
                    CHECK (supervisor_kind <> ''),
                    CHECK (status IN ('starting', 'running', 'stale', 'stopped', 'failed')),
                    CHECK (transport_kind <> ''),
                    CHECK (transport_endpoint <> ''),
                    CHECK (transport_liveness IN ('unknown', 'reachable', 'unreachable')),
                    CHECK (control_state_json <> ''),
                    CHECK (
                        (last_error_code IS NULL AND last_error_message IS NULL)
                        OR (last_error_code IS NOT NULL AND last_error_message IS NOT NULL)
                    ),
                    FOREIGN KEY (project_id, agent_session_id)
                        REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE
                );

                INSERT INTO runtime_runs_session_scoped (
                    project_id,
                    agent_session_id,
                    run_id,
                    runtime_kind,
                    provider_id,
                    supervisor_kind,
                    status,
                    transport_kind,
                    transport_endpoint,
                    transport_liveness,
                    control_state_json,
                    last_checkpoint_sequence,
                    started_at,
                    last_heartbeat_at,
                    last_checkpoint_at,
                    stopped_at,
                    last_error_code,
                    last_error_message,
                    updated_at,
                    created_at
                )
                SELECT
                    project_id,
                    'agent-session-main',
                    run_id,
                    runtime_kind,
                    provider_id,
                    supervisor_kind,
                    status,
                    transport_kind,
                    transport_endpoint,
                    transport_liveness,
                    control_state_json,
                    last_checkpoint_sequence,
                    started_at,
                    last_heartbeat_at,
                    last_checkpoint_at,
                    stopped_at,
                    last_error_code,
                    last_error_message,
                    updated_at,
                    created_at
                FROM runtime_runs
                WHERE provider_id IS NOT NULL
                  AND control_state_json IS NOT NULL;

                CREATE TABLE runtime_run_checkpoints_session_scoped (
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
                        REFERENCES runtime_runs_session_scoped(project_id, run_id) ON DELETE CASCADE
                );

                INSERT INTO runtime_run_checkpoints_session_scoped (
                    id,
                    project_id,
                    run_id,
                    sequence,
                    kind,
                    summary,
                    created_at
                )
                SELECT
                    id,
                    project_id,
                    run_id,
                    sequence,
                    kind,
                    summary,
                    created_at
                FROM runtime_run_checkpoints;

                CREATE TABLE autonomous_runs_session_scoped (
                    project_id TEXT NOT NULL,
                    agent_session_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    runtime_kind TEXT NOT NULL,
                    provider_id TEXT NOT NULL,
                    supervisor_kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    active_unit_sequence INTEGER,
                    duplicate_start_detected INTEGER NOT NULL DEFAULT 0 CHECK (duplicate_start_detected IN (0, 1)),
                    duplicate_start_run_id TEXT,
                    duplicate_start_reason TEXT,
                    started_at TEXT NOT NULL,
                    last_heartbeat_at TEXT,
                    last_checkpoint_at TEXT,
                    paused_at TEXT,
                    cancelled_at TEXT,
                    completed_at TEXT,
                    crashed_at TEXT,
                    stopped_at TEXT,
                    pause_reason_code TEXT,
                    pause_reason_message TEXT,
                    cancel_reason_code TEXT,
                    cancel_reason_message TEXT,
                    crash_reason_code TEXT,
                    crash_reason_message TEXT,
                    last_error_code TEXT,
                    last_error_message TEXT,
                    updated_at TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    PRIMARY KEY (project_id, agent_session_id),
                    UNIQUE (project_id, run_id),
                    CHECK (agent_session_id <> ''),
                    CHECK (run_id <> ''),
                    CHECK (runtime_kind <> ''),
                    CHECK (provider_id <> ''),
                    CHECK (supervisor_kind <> ''),
                    CHECK (status IN ('starting', 'running', 'paused', 'cancelling', 'cancelled', 'stale', 'failed', 'stopped', 'crashed', 'completed')),
                    CHECK (active_unit_sequence IS NULL OR active_unit_sequence > 0),
                    CHECK (duplicate_start_run_id IS NULL OR duplicate_start_run_id <> ''),
                    CHECK (duplicate_start_reason IS NULL OR duplicate_start_reason <> ''),
                    CHECK (
                        (pause_reason_code IS NULL AND pause_reason_message IS NULL)
                        OR (pause_reason_code IS NOT NULL AND pause_reason_message IS NOT NULL)
                    ),
                    CHECK (
                        (cancel_reason_code IS NULL AND cancel_reason_message IS NULL)
                        OR (cancel_reason_code IS NOT NULL AND cancel_reason_message IS NOT NULL)
                    ),
                    CHECK (
                        (crash_reason_code IS NULL AND crash_reason_message IS NULL)
                        OR (crash_reason_code IS NOT NULL AND crash_reason_message IS NOT NULL)
                    ),
                    CHECK (
                        (last_error_code IS NULL AND last_error_message IS NULL)
                        OR (last_error_code IS NOT NULL AND last_error_message IS NOT NULL)
                    ),
                    FOREIGN KEY (project_id, agent_session_id)
                        REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
                    FOREIGN KEY (project_id, run_id)
                        REFERENCES runtime_runs_session_scoped(project_id, run_id) ON DELETE CASCADE
                );

                INSERT INTO autonomous_runs_session_scoped (
                    project_id,
                    agent_session_id,
                    run_id,
                    runtime_kind,
                    provider_id,
                    supervisor_kind,
                    status,
                    active_unit_sequence,
                    duplicate_start_detected,
                    duplicate_start_run_id,
                    duplicate_start_reason,
                    started_at,
                    last_heartbeat_at,
                    last_checkpoint_at,
                    paused_at,
                    cancelled_at,
                    completed_at,
                    crashed_at,
                    stopped_at,
                    pause_reason_code,
                    pause_reason_message,
                    cancel_reason_code,
                    cancel_reason_message,
                    crash_reason_code,
                    crash_reason_message,
                    last_error_code,
                    last_error_message,
                    updated_at,
                    created_at
                )
                SELECT
                    project_id,
                    'agent-session-main',
                    run_id,
                    runtime_kind,
                    provider_id,
                    supervisor_kind,
                    status,
                    active_unit_sequence,
                    duplicate_start_detected,
                    duplicate_start_run_id,
                    duplicate_start_reason,
                    started_at,
                    last_heartbeat_at,
                    last_checkpoint_at,
                    paused_at,
                    cancelled_at,
                    completed_at,
                    crashed_at,
                    stopped_at,
                    pause_reason_code,
                    pause_reason_message,
                    cancel_reason_code,
                    cancel_reason_message,
                    crash_reason_code,
                    crash_reason_message,
                    last_error_code,
                    last_error_message,
                    updated_at,
                    created_at
                FROM autonomous_runs
                WHERE provider_id IS NOT NULL;

                CREATE TABLE autonomous_units_session_scoped (
                    unit_id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    sequence INTEGER NOT NULL CHECK (sequence > 0),
                    kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    summary TEXT NOT NULL,
                    boundary_id TEXT,
                    workflow_node_id TEXT,
                    workflow_transition_id TEXT,
                    workflow_causal_transition_id TEXT,
                    workflow_handoff_transition_id TEXT,
                    workflow_handoff_package_hash TEXT,
                    started_at TEXT NOT NULL,
                    finished_at TEXT,
                    last_error_code TEXT,
                    last_error_message TEXT,
                    updated_at TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    CHECK (unit_id <> ''),
                    CHECK (kind <> ''),
                    CHECK (status IN ('pending', 'active', 'blocked', 'paused', 'completed', 'cancelled', 'failed')),
                    CHECK (summary <> ''),
                    CHECK (boundary_id IS NULL OR boundary_id <> ''),
                    CHECK (
                        (last_error_code IS NULL AND last_error_message IS NULL)
                        OR (last_error_code IS NOT NULL AND last_error_message IS NOT NULL)
                    ),
                    UNIQUE (project_id, run_id, sequence),
                    UNIQUE (project_id, run_id, unit_id),
                    FOREIGN KEY (project_id, run_id)
                        REFERENCES autonomous_runs_session_scoped(project_id, run_id) ON DELETE CASCADE
                );

                INSERT INTO autonomous_units_session_scoped (
                    unit_id,
                    project_id,
                    run_id,
                    sequence,
                    kind,
                    status,
                    summary,
                    boundary_id,
                    workflow_node_id,
                    workflow_transition_id,
                    workflow_causal_transition_id,
                    workflow_handoff_transition_id,
                    workflow_handoff_package_hash,
                    started_at,
                    finished_at,
                    last_error_code,
                    last_error_message,
                    updated_at,
                    created_at
                )
                SELECT
                    unit_id,
                    project_id,
                    run_id,
                    sequence,
                    kind,
                    status,
                    summary,
                    boundary_id,
                    workflow_node_id,
                    workflow_transition_id,
                    workflow_causal_transition_id,
                    workflow_handoff_transition_id,
                    workflow_handoff_package_hash,
                    started_at,
                    finished_at,
                    last_error_code,
                    last_error_message,
                    updated_at,
                    created_at
                FROM autonomous_units;

                CREATE TABLE autonomous_unit_attempts_session_scoped (
                    attempt_id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    unit_id TEXT NOT NULL,
                    attempt_number INTEGER NOT NULL CHECK (attempt_number > 0),
                    child_session_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    boundary_id TEXT,
                    workflow_node_id TEXT,
                    workflow_transition_id TEXT,
                    workflow_causal_transition_id TEXT,
                    workflow_handoff_transition_id TEXT,
                    workflow_handoff_package_hash TEXT,
                    started_at TEXT NOT NULL,
                    finished_at TEXT,
                    last_error_code TEXT,
                    last_error_message TEXT,
                    updated_at TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    CHECK (attempt_id <> ''),
                    CHECK (child_session_id <> ''),
                    CHECK (status IN ('pending', 'active', 'blocked', 'paused', 'completed', 'cancelled', 'failed')),
                    CHECK (boundary_id IS NULL OR boundary_id <> ''),
                    CHECK (
                        (last_error_code IS NULL AND last_error_message IS NULL)
                        OR (last_error_code IS NOT NULL AND last_error_message IS NOT NULL)
                    ),
                    UNIQUE (project_id, run_id, unit_id, attempt_number),
                    UNIQUE (project_id, run_id, child_session_id),
                    UNIQUE (project_id, run_id, attempt_id),
                    FOREIGN KEY (project_id, run_id, unit_id)
                        REFERENCES autonomous_units_session_scoped(project_id, run_id, unit_id) ON DELETE CASCADE
                );

                INSERT INTO autonomous_unit_attempts_session_scoped (
                    attempt_id,
                    project_id,
                    run_id,
                    unit_id,
                    attempt_number,
                    child_session_id,
                    status,
                    boundary_id,
                    workflow_node_id,
                    workflow_transition_id,
                    workflow_causal_transition_id,
                    workflow_handoff_transition_id,
                    workflow_handoff_package_hash,
                    started_at,
                    finished_at,
                    last_error_code,
                    last_error_message,
                    updated_at,
                    created_at
                )
                SELECT
                    attempt_id,
                    project_id,
                    run_id,
                    unit_id,
                    attempt_number,
                    child_session_id,
                    status,
                    boundary_id,
                    workflow_node_id,
                    workflow_transition_id,
                    workflow_causal_transition_id,
                    workflow_handoff_transition_id,
                    workflow_handoff_package_hash,
                    started_at,
                    finished_at,
                    last_error_code,
                    last_error_message,
                    updated_at,
                    created_at
                FROM autonomous_unit_attempts;

                CREATE TABLE autonomous_unit_artifacts_session_scoped (
                    artifact_id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    unit_id TEXT NOT NULL,
                    attempt_id TEXT NOT NULL,
                    artifact_kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    summary TEXT NOT NULL,
                    content_hash TEXT,
                    payload_json TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    CHECK (artifact_id <> ''),
                    CHECK (artifact_kind <> ''),
                    CHECK (status IN ('pending', 'recorded', 'rejected', 'redacted')),
                    CHECK (summary <> ''),
                    CHECK (content_hash IS NULL OR (length(content_hash) = 64 AND content_hash NOT GLOB '*[^0-9a-f]*')),
                    UNIQUE (project_id, run_id, artifact_id),
                    FOREIGN KEY (project_id, run_id, unit_id)
                        REFERENCES autonomous_units_session_scoped(project_id, run_id, unit_id) ON DELETE CASCADE,
                    FOREIGN KEY (project_id, run_id, attempt_id)
                        REFERENCES autonomous_unit_attempts_session_scoped(project_id, run_id, attempt_id) ON DELETE CASCADE
                );

                INSERT INTO autonomous_unit_artifacts_session_scoped (
                    artifact_id,
                    project_id,
                    run_id,
                    unit_id,
                    attempt_id,
                    artifact_kind,
                    status,
                    summary,
                    content_hash,
                    payload_json,
                    created_at,
                    updated_at
                )
                SELECT
                    artifact_id,
                    project_id,
                    run_id,
                    unit_id,
                    attempt_id,
                    artifact_kind,
                    status,
                    summary,
                    content_hash,
                    payload_json,
                    created_at,
                    updated_at
                FROM autonomous_unit_artifacts;

                DROP TABLE autonomous_unit_artifacts;
                DROP TABLE autonomous_unit_attempts;
                DROP TABLE autonomous_units;
                DROP TABLE autonomous_runs;
                DROP TABLE runtime_run_checkpoints;
                DROP TABLE runtime_runs;

                ALTER TABLE runtime_runs_session_scoped RENAME TO runtime_runs;
                ALTER TABLE runtime_run_checkpoints_session_scoped RENAME TO runtime_run_checkpoints;
                ALTER TABLE autonomous_runs_session_scoped RENAME TO autonomous_runs;
                ALTER TABLE autonomous_units_session_scoped RENAME TO autonomous_units;
                ALTER TABLE autonomous_unit_attempts_session_scoped RENAME TO autonomous_unit_attempts;
                ALTER TABLE autonomous_unit_artifacts_session_scoped RENAME TO autonomous_unit_artifacts;

                CREATE UNIQUE INDEX IF NOT EXISTS idx_runtime_runs_project_run
                    ON runtime_runs(project_id, run_id);
                CREATE INDEX IF NOT EXISTS idx_runtime_runs_status_updated
                    ON runtime_runs(project_id, agent_session_id, status, updated_at DESC);
                CREATE INDEX IF NOT EXISTS idx_runtime_runs_supervisor_kind
                    ON runtime_runs(supervisor_kind, transport_liveness);
                CREATE INDEX IF NOT EXISTS idx_runtime_runs_provider_status_updated
                    ON runtime_runs(provider_id, status, updated_at DESC);

                CREATE INDEX IF NOT EXISTS idx_runtime_run_checkpoints_project_run_sequence
                    ON runtime_run_checkpoints(project_id, run_id, sequence DESC);
                CREATE INDEX IF NOT EXISTS idx_runtime_run_checkpoints_project_created
                    ON runtime_run_checkpoints(project_id, created_at DESC, id DESC);

                CREATE UNIQUE INDEX IF NOT EXISTS idx_autonomous_runs_project_run
                    ON autonomous_runs(project_id, run_id);
                CREATE INDEX IF NOT EXISTS idx_autonomous_runs_status_updated
                    ON autonomous_runs(project_id, agent_session_id, status, updated_at DESC);
                CREATE INDEX IF NOT EXISTS idx_autonomous_runs_provider_status_updated
                    ON autonomous_runs(provider_id, status, updated_at DESC);

                CREATE UNIQUE INDEX IF NOT EXISTS idx_autonomous_units_one_active_per_run
                    ON autonomous_units(project_id, run_id)
                    WHERE status = 'active';
                CREATE INDEX IF NOT EXISTS idx_autonomous_units_project_run_sequence
                    ON autonomous_units(project_id, run_id, sequence DESC, updated_at DESC);

                CREATE UNIQUE INDEX IF NOT EXISTS idx_autonomous_unit_attempts_one_active_per_run
                    ON autonomous_unit_attempts(project_id, run_id)
                    WHERE status = 'active';
                CREATE INDEX IF NOT EXISTS idx_autonomous_unit_attempts_project_run_unit_attempt
                    ON autonomous_unit_attempts(project_id, run_id, unit_id, attempt_number DESC, updated_at DESC);

                CREATE INDEX IF NOT EXISTS idx_autonomous_unit_artifacts_project_run_attempt
                    ON autonomous_unit_artifacts(project_id, run_id, attempt_id, created_at DESC, artifact_id ASC);
                "#,
            ),
            M::up(
                r#"
                CREATE TABLE IF NOT EXISTS agent_runs (
                    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                    agent_session_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    provider_id TEXT NOT NULL,
                    model_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    prompt TEXT NOT NULL,
                    system_prompt TEXT NOT NULL,
                    started_at TEXT NOT NULL,
                    last_heartbeat_at TEXT,
                    completed_at TEXT,
                    cancelled_at TEXT,
                    last_error_code TEXT,
                    last_error_message TEXT,
                    updated_at TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    PRIMARY KEY (project_id, run_id),
                    CHECK (agent_session_id <> ''),
                    CHECK (run_id <> ''),
                    CHECK (provider_id <> ''),
                    CHECK (model_id <> ''),
                    CHECK (status IN ('starting', 'running', 'cancelling', 'cancelled', 'completed', 'failed')),
                    CHECK (prompt <> ''),
                    CHECK (system_prompt <> ''),
                    CHECK (
                        (last_error_code IS NULL AND last_error_message IS NULL)
                        OR (last_error_code IS NOT NULL AND last_error_message IS NOT NULL)
                    ),
                    FOREIGN KEY (project_id, agent_session_id)
                        REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_agent_runs_session_updated
                    ON agent_runs(project_id, agent_session_id, updated_at DESC, started_at DESC);
                CREATE INDEX IF NOT EXISTS idx_agent_runs_status_updated
                    ON agent_runs(project_id, status, updated_at DESC);

                CREATE TABLE IF NOT EXISTS agent_messages (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    content TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    CHECK (role IN ('system', 'developer', 'user', 'assistant', 'tool')),
                    CHECK (content <> ''),
                    FOREIGN KEY (project_id, run_id)
                        REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_agent_messages_project_run_id
                    ON agent_messages(project_id, run_id, id ASC);

                CREATE TABLE IF NOT EXISTS agent_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    event_kind TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    CHECK (event_kind IN (
                        'message_delta',
                        'reasoning_summary',
                        'tool_started',
                        'tool_delta',
                        'tool_completed',
                        'file_changed',
                        'command_output',
                        'validation_started',
                        'validation_completed',
                        'action_required',
                        'run_completed',
                        'run_failed'
                    )),
                    CHECK (payload_json <> ''),
                    CHECK (json_valid(payload_json)),
                    FOREIGN KEY (project_id, run_id)
                        REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_agent_events_project_run_id
                    ON agent_events(project_id, run_id, id ASC);

                CREATE TABLE IF NOT EXISTS agent_tool_calls (
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    tool_call_id TEXT NOT NULL,
                    tool_name TEXT NOT NULL,
                    input_json TEXT NOT NULL,
                    state TEXT NOT NULL,
                    result_json TEXT,
                    error_code TEXT,
                    error_message TEXT,
                    started_at TEXT NOT NULL,
                    completed_at TEXT,
                    PRIMARY KEY (project_id, run_id, tool_call_id),
                    CHECK (tool_call_id <> ''),
                    CHECK (tool_name <> ''),
                    CHECK (input_json <> ''),
                    CHECK (json_valid(input_json)),
                    CHECK (state IN ('pending', 'running', 'succeeded', 'failed')),
                    CHECK (result_json IS NULL OR (result_json <> '' AND json_valid(result_json))),
                    CHECK (
                        (error_code IS NULL AND error_message IS NULL)
                        OR (error_code IS NOT NULL AND error_message IS NOT NULL)
                    ),
                    FOREIGN KEY (project_id, run_id)
                        REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_agent_tool_calls_project_run_started
                    ON agent_tool_calls(project_id, run_id, started_at ASC, tool_call_id ASC);

                CREATE TABLE IF NOT EXISTS agent_file_changes (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    path TEXT NOT NULL,
                    operation TEXT NOT NULL,
                    old_hash TEXT,
                    new_hash TEXT,
                    created_at TEXT NOT NULL,
                    CHECK (path <> ''),
                    CHECK (operation IN ('create', 'write', 'edit', 'patch', 'delete', 'rename', 'mkdir', 'unknown')),
                    CHECK (old_hash IS NULL OR (length(old_hash) = 64 AND old_hash NOT GLOB '*[^0-9a-f]*')),
                    CHECK (new_hash IS NULL OR (length(new_hash) = 64 AND new_hash NOT GLOB '*[^0-9a-f]*')),
                    FOREIGN KEY (project_id, run_id)
                        REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_agent_file_changes_project_run_path
                    ON agent_file_changes(project_id, run_id, path, id ASC);

                CREATE TABLE IF NOT EXISTS agent_checkpoints (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    checkpoint_kind TEXT NOT NULL,
                    summary TEXT NOT NULL,
                    payload_json TEXT,
                    created_at TEXT NOT NULL,
                    CHECK (checkpoint_kind IN ('preflight', 'message', 'tool', 'validation', 'completion', 'failure', 'recovery')),
                    CHECK (summary <> ''),
                    CHECK (payload_json IS NULL OR (payload_json <> '' AND json_valid(payload_json))),
                    FOREIGN KEY (project_id, run_id)
                        REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_agent_checkpoints_project_run_id
                    ON agent_checkpoints(project_id, run_id, id ASC);

                CREATE TABLE IF NOT EXISTS agent_usage (
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    provider_id TEXT NOT NULL,
                    model_id TEXT NOT NULL,
                    input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (input_tokens >= 0),
                    output_tokens INTEGER NOT NULL DEFAULT 0 CHECK (output_tokens >= 0),
                    total_tokens INTEGER NOT NULL DEFAULT 0 CHECK (total_tokens >= 0),
                    estimated_cost_micros INTEGER NOT NULL DEFAULT 0 CHECK (estimated_cost_micros >= 0),
                    updated_at TEXT NOT NULL,
                    PRIMARY KEY (project_id, run_id),
                    CHECK (provider_id <> ''),
                    CHECK (model_id <> ''),
                    FOREIGN KEY (project_id, run_id)
                        REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS agent_action_requests (
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    action_id TEXT NOT NULL,
                    action_type TEXT NOT NULL,
                    title TEXT NOT NULL,
                    detail TEXT NOT NULL,
                    status TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    resolved_at TEXT,
                    response TEXT,
                    PRIMARY KEY (project_id, run_id, action_id),
                    CHECK (action_id <> ''),
                    CHECK (action_type <> ''),
                    CHECK (title <> ''),
                    CHECK (detail <> ''),
                    CHECK (status IN ('pending', 'approved', 'rejected', 'answered', 'cancelled')),
                    CHECK (
                        (status = 'pending' AND resolved_at IS NULL)
                        OR (status <> 'pending' AND resolved_at IS NOT NULL)
                    ),
                    FOREIGN KEY (project_id, run_id)
                        REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_agent_action_requests_project_status
                    ON agent_action_requests(project_id, status, created_at DESC);
                "#,
            ),
            M::up(
                r#"
                CREATE TABLE IF NOT EXISTS installed_skill_records (
                    source_id TEXT PRIMARY KEY,
                    scope_kind TEXT NOT NULL,
                    project_id TEXT,
                    contract_version INTEGER NOT NULL,
                    skill_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    description TEXT NOT NULL,
                    user_invocable INTEGER,
                    source_state TEXT NOT NULL,
                    trust_state TEXT NOT NULL,
                    source_json TEXT NOT NULL,
                    cache_key TEXT,
                    local_location TEXT,
                    version_hash TEXT,
                    installed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    last_used_at TEXT,
                    last_diagnostic_json TEXT,
                    CHECK (scope_kind IN ('global', 'project')),
                    CHECK (
                        (scope_kind = 'global' AND project_id IS NULL)
                        OR (scope_kind = 'project' AND project_id IS NOT NULL AND project_id <> '')
                    ),
                    CHECK (contract_version > 0),
                    CHECK (skill_id <> ''),
                    CHECK (name <> ''),
                    CHECK (description <> ''),
                    CHECK (user_invocable IS NULL OR user_invocable IN (0, 1)),
                    CHECK (source_state IN ('installed', 'enabled', 'disabled', 'stale', 'failed', 'blocked')),
                    CHECK (trust_state IN ('trusted', 'user_approved', 'approval_required', 'untrusted', 'blocked')),
                    CHECK (source_json <> '' AND json_valid(source_json)),
                    CHECK (cache_key IS NULL OR cache_key <> ''),
                    CHECK (local_location IS NULL OR local_location <> ''),
                    CHECK (cache_key IS NOT NULL OR local_location IS NOT NULL),
                    CHECK (version_hash IS NULL OR version_hash <> ''),
                    CHECK (last_used_at IS NULL OR last_used_at <> ''),
                    CHECK (last_diagnostic_json IS NULL OR (last_diagnostic_json <> '' AND json_valid(last_diagnostic_json))),
                    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_installed_skill_records_scope_skill
                    ON installed_skill_records(scope_kind, project_id, skill_id, source_id);
                CREATE INDEX IF NOT EXISTS idx_installed_skill_records_state_updated
                    ON installed_skill_records(source_state, updated_at DESC);
                "#,
            ),
            M::up(
                r#"
                DROP TABLE IF EXISTS workflow_handoff_packages;
                DROP TABLE IF EXISTS workflow_transition_events;
                DROP TABLE IF EXISTS workflow_gate_metadata;
                DROP TABLE IF EXISTS workflow_graph_edges;
                DROP TABLE IF EXISTS workflow_graph_nodes;
                DROP TABLE IF EXISTS workflow_phases;

                DROP TABLE IF EXISTS autonomous_unit_artifacts;
                DROP TABLE IF EXISTS autonomous_unit_attempts;
                DROP TABLE IF EXISTS autonomous_units;
                "#,
            ),
            M::up(
                r#"
                CREATE TABLE IF NOT EXISTS installed_plugin_records (
                    plugin_id TEXT PRIMARY KEY,
                    root_id TEXT NOT NULL,
                    root_path TEXT NOT NULL,
                    plugin_root_path TEXT NOT NULL,
                    manifest_path TEXT NOT NULL,
                    manifest_hash TEXT NOT NULL,
                    name TEXT NOT NULL,
                    version TEXT NOT NULL,
                    description TEXT NOT NULL,
                    plugin_state TEXT NOT NULL,
                    trust_state TEXT NOT NULL,
                    manifest_json TEXT NOT NULL,
                    installed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    last_reloaded_at TEXT,
                    last_diagnostic_json TEXT,
                    CHECK (plugin_id <> ''),
                    CHECK (root_id <> ''),
                    CHECK (root_path <> ''),
                    CHECK (plugin_root_path <> ''),
                    CHECK (manifest_path <> ''),
                    CHECK (length(manifest_hash) = 64),
                    CHECK (manifest_hash NOT GLOB '*[^0-9a-f]*'),
                    CHECK (name <> ''),
                    CHECK (version <> ''),
                    CHECK (description <> ''),
                    CHECK (plugin_state IN ('installed', 'enabled', 'disabled', 'stale', 'failed', 'blocked')),
                    CHECK (trust_state IN ('trusted', 'user_approved', 'approval_required', 'untrusted', 'blocked')),
                    CHECK (manifest_json <> '' AND json_valid(manifest_json)),
                    CHECK (last_reloaded_at IS NULL OR last_reloaded_at <> ''),
                    CHECK (last_diagnostic_json IS NULL OR (last_diagnostic_json <> '' AND json_valid(last_diagnostic_json)))
                );

                CREATE INDEX IF NOT EXISTS idx_installed_plugin_records_state_updated
                    ON installed_plugin_records(plugin_state, updated_at DESC);
                CREATE INDEX IF NOT EXISTS idx_installed_plugin_records_root
                    ON installed_plugin_records(root_id, plugin_id);
                "#,
            ),
            M::up(
                r#"
                CREATE TABLE IF NOT EXISTS agent_compactions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    compaction_id TEXT NOT NULL,
                    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                    agent_session_id TEXT NOT NULL,
                    source_run_id TEXT NOT NULL,
                    provider_id TEXT NOT NULL,
                    model_id TEXT NOT NULL,
                    summary TEXT NOT NULL,
                    covered_run_ids_json TEXT NOT NULL,
                    covered_message_start_id INTEGER,
                    covered_message_end_id INTEGER,
                    covered_event_start_id INTEGER,
                    covered_event_end_id INTEGER,
                    source_hash TEXT NOT NULL,
                    input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (input_tokens >= 0),
                    summary_tokens INTEGER NOT NULL DEFAULT 0 CHECK (summary_tokens >= 0),
                    raw_tail_message_count INTEGER NOT NULL DEFAULT 0 CHECK (raw_tail_message_count >= 0),
                    policy_reason TEXT NOT NULL,
                    trigger_kind TEXT NOT NULL,
                    active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
                    diagnostic_json TEXT,
                    created_at TEXT NOT NULL,
                    superseded_at TEXT,
                    CHECK (compaction_id <> ''),
                    CHECK (agent_session_id <> ''),
                    CHECK (source_run_id <> ''),
                    CHECK (provider_id <> ''),
                    CHECK (model_id <> ''),
                    CHECK (summary <> ''),
                    CHECK (covered_run_ids_json <> '' AND json_valid(covered_run_ids_json)),
                    CHECK (
                        (covered_message_start_id IS NULL AND covered_message_end_id IS NULL)
                        OR (
                            covered_message_start_id IS NOT NULL
                            AND covered_message_end_id IS NOT NULL
                            AND covered_message_start_id <= covered_message_end_id
                        )
                    ),
                    CHECK (
                        (covered_event_start_id IS NULL AND covered_event_end_id IS NULL)
                        OR (
                            covered_event_start_id IS NOT NULL
                            AND covered_event_end_id IS NOT NULL
                            AND covered_event_start_id <= covered_event_end_id
                        )
                    ),
                    CHECK (length(source_hash) = 64),
                    CHECK (source_hash NOT GLOB '*[^0-9a-f]*'),
                    CHECK (policy_reason <> ''),
                    CHECK (trigger_kind IN ('manual', 'auto')),
                    CHECK (diagnostic_json IS NULL OR (diagnostic_json <> '' AND json_valid(diagnostic_json))),
                    CHECK (superseded_at IS NULL OR superseded_at <> ''),
                    UNIQUE (project_id, compaction_id),
                    FOREIGN KEY (project_id, agent_session_id)
                        REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
                    FOREIGN KEY (project_id, source_run_id)
                        REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
                );

                CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_compactions_one_active
                    ON agent_compactions(project_id, agent_session_id)
                    WHERE active = 1;
                CREATE INDEX IF NOT EXISTS idx_agent_compactions_session_created
                    ON agent_compactions(project_id, agent_session_id, created_at DESC, id DESC);
                CREATE INDEX IF NOT EXISTS idx_agent_compactions_source_run
                    ON agent_compactions(project_id, source_run_id, active);
                "#,
            ),
            M::up(
                r#"
                CREATE TABLE IF NOT EXISTS agent_memories (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    memory_id TEXT NOT NULL,
                    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                    agent_session_id TEXT,
                    scope_kind TEXT NOT NULL,
                    memory_kind TEXT NOT NULL,
                    text TEXT NOT NULL,
                    text_hash TEXT NOT NULL,
                    review_state TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
                    confidence INTEGER CHECK (confidence IS NULL OR (confidence >= 0 AND confidence <= 100)),
                    source_run_id TEXT,
                    source_item_ids_json TEXT NOT NULL DEFAULT '[]',
                    diagnostic_json TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    CHECK (memory_id <> ''),
                    CHECK (agent_session_id IS NULL OR agent_session_id <> ''),
                    CHECK (scope_kind IN ('project', 'session')),
                    CHECK (memory_kind IN ('project_fact', 'user_preference', 'decision', 'session_summary', 'troubleshooting')),
                    CHECK (text <> ''),
                    CHECK (length(text_hash) = 64),
                    CHECK (text_hash NOT GLOB '*[^0-9a-f]*'),
                    CHECK (review_state IN ('candidate', 'approved', 'rejected')),
                    CHECK (
                        (scope_kind = 'project' AND agent_session_id IS NULL)
                        OR (scope_kind = 'session' AND agent_session_id IS NOT NULL)
                    ),
                    CHECK (review_state = 'approved' OR enabled = 0),
                    CHECK (source_run_id IS NULL OR source_run_id <> ''),
                    CHECK (source_item_ids_json <> '' AND json_valid(source_item_ids_json)),
                    CHECK (diagnostic_json IS NULL OR (diagnostic_json <> '' AND json_valid(diagnostic_json))),
                    UNIQUE (project_id, memory_id),
                    FOREIGN KEY (project_id, agent_session_id)
                        REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_agent_memories_scope_review
                    ON agent_memories(project_id, scope_kind, agent_session_id, review_state, enabled, updated_at DESC);
                CREATE INDEX IF NOT EXISTS idx_agent_memories_source_run
                    ON agent_memories(project_id, source_run_id)
                    WHERE source_run_id IS NOT NULL;
                CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_memories_active_text
                    ON agent_memories(project_id, scope_kind, COALESCE(agent_session_id, ''), memory_kind, text_hash)
                    WHERE review_state IN ('candidate', 'approved');

                CREATE TRIGGER IF NOT EXISTS agent_memories_clear_deleted_source_run
                AFTER DELETE ON agent_runs
                BEGIN
                    UPDATE agent_memories
                    SET source_run_id = NULL,
                        source_item_ids_json = '[]',
                        diagnostic_json = json_object(
                            'code', 'memory_source_deleted',
                            'message', 'The source run for this memory was deleted, so Cadence cleared its provenance reference.'
                        ),
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                    WHERE project_id = old.project_id
                      AND source_run_id = old.run_id;
                END;

                CREATE TRIGGER IF NOT EXISTS agent_memories_clear_deleted_session_sources
                BEFORE DELETE ON agent_sessions
                BEGIN
                    UPDATE agent_memories
                    SET source_run_id = NULL,
                        source_item_ids_json = '[]',
                        diagnostic_json = json_object(
                            'code', 'memory_source_deleted',
                            'message', 'The source session for this memory was deleted, so Cadence cleared its provenance reference.'
                        ),
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                    WHERE project_id = old.project_id
                      AND source_run_id IN (
                        SELECT run_id
                        FROM agent_runs
                        WHERE project_id = old.project_id
                          AND agent_session_id = old.agent_session_id
                      );
                END;
                "#,
            ),
            M::up(
                r#"
                CREATE TABLE IF NOT EXISTS agent_session_lineage (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    lineage_id TEXT NOT NULL,
                    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                    child_agent_session_id TEXT NOT NULL,
                    source_agent_session_id TEXT,
                    source_run_id TEXT,
                    source_boundary_kind TEXT NOT NULL,
                    source_message_id INTEGER,
                    source_checkpoint_id INTEGER,
                    source_compaction_id TEXT,
                    source_title TEXT NOT NULL,
                    branch_title TEXT NOT NULL,
                    replay_run_id TEXT NOT NULL,
                    file_change_summary TEXT NOT NULL DEFAULT '',
                    diagnostic_json TEXT,
                    created_at TEXT NOT NULL,
                    source_deleted_at TEXT,
                    CHECK (lineage_id <> ''),
                    CHECK (child_agent_session_id <> ''),
                    CHECK (source_agent_session_id IS NULL OR source_agent_session_id <> ''),
                    CHECK (source_run_id IS NULL OR source_run_id <> ''),
                    CHECK (source_boundary_kind IN ('run', 'message', 'checkpoint')),
                    CHECK (source_compaction_id IS NULL OR source_compaction_id <> ''),
                    CHECK (source_title <> ''),
                    CHECK (branch_title <> ''),
                    CHECK (replay_run_id <> ''),
                    CHECK (diagnostic_json IS NULL OR (diagnostic_json <> '' AND json_valid(diagnostic_json))),
                    CHECK (source_deleted_at IS NULL OR source_deleted_at <> ''),
                    CHECK (
                        source_deleted_at IS NOT NULL
                        OR (source_agent_session_id IS NOT NULL AND source_run_id IS NOT NULL)
                    ),
                    CHECK (
                        (source_boundary_kind = 'run' AND source_message_id IS NULL AND source_checkpoint_id IS NULL)
                        OR (source_boundary_kind = 'message' AND source_message_id IS NOT NULL AND source_checkpoint_id IS NULL)
                        OR (source_boundary_kind = 'checkpoint' AND source_message_id IS NULL AND source_checkpoint_id IS NOT NULL)
                    ),
                    UNIQUE (project_id, lineage_id),
                    UNIQUE (project_id, child_agent_session_id),
                    FOREIGN KEY (project_id, child_agent_session_id)
                        REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
                    FOREIGN KEY (project_id, replay_run_id)
                        REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_agent_session_lineage_source
                    ON agent_session_lineage(project_id, source_agent_session_id, source_run_id);
                CREATE INDEX IF NOT EXISTS idx_agent_session_lineage_replay_run
                    ON agent_session_lineage(project_id, replay_run_id);

                CREATE TRIGGER IF NOT EXISTS agent_session_lineage_mark_deleted_source_run
                AFTER DELETE ON agent_runs
                BEGIN
                    UPDATE agent_session_lineage
                    SET source_agent_session_id = NULL,
                        source_run_id = NULL,
                        source_message_id = NULL,
                        source_checkpoint_id = NULL,
                        source_compaction_id = NULL,
                        source_deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                        diagnostic_json = json_object(
                            'code', 'branch_source_deleted',
                            'message', 'The source run for this branch was deleted. Cadence preserved the branch replay copy and cleared the source reference.'
                        )
                    WHERE project_id = old.project_id
                      AND source_run_id = old.run_id;
                END;

                CREATE TRIGGER IF NOT EXISTS agent_session_lineage_mark_deleted_source_session
                BEFORE DELETE ON agent_sessions
                BEGIN
                    UPDATE agent_session_lineage
                    SET source_agent_session_id = NULL,
                        source_run_id = NULL,
                        source_message_id = NULL,
                        source_checkpoint_id = NULL,
                        source_compaction_id = NULL,
                        source_deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                        diagnostic_json = json_object(
                            'code', 'branch_source_deleted',
                            'message', 'The source session for this branch was deleted. Cadence preserved the branch replay copy and cleared the source reference.'
                        )
                    WHERE project_id = old.project_id
                      AND source_agent_session_id = old.agent_session_id;
                END;
                "#,
            ),
            M::up(
                r#"
                ALTER TABLE agent_usage
                    ADD COLUMN cache_read_tokens INTEGER NOT NULL DEFAULT 0
                    CHECK (cache_read_tokens >= 0);
                ALTER TABLE agent_usage
                    ADD COLUMN cache_creation_tokens INTEGER NOT NULL DEFAULT 0
                    CHECK (cache_creation_tokens >= 0);

                CREATE INDEX IF NOT EXISTS idx_agent_usage_project_model
                    ON agent_usage(project_id, provider_id, model_id);
                "#,
            ),
        ])
    });

    &MIGRATIONS
}
