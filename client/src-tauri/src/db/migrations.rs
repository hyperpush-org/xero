use std::sync::LazyLock;

use rusqlite_migration::{Migrations, M};

pub fn migrations() -> &'static Migrations<'static> {
    static MIGRATIONS: LazyLock<Migrations<'static>> = LazyLock::new(|| {
        Migrations::new(vec![
            M::up(BASELINE_SCHEMA_SQL),
            M::up(MIGRATION_001_AGENT_MESSAGE_ATTACHMENTS_SQL),
        ])
    });

    &MIGRATIONS
}

const MIGRATION_001_AGENT_MESSAGE_ATTACHMENTS_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS agent_message_attachments (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        message_id INTEGER NOT NULL REFERENCES agent_messages(id) ON DELETE CASCADE,
        project_id TEXT NOT NULL,
        run_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        storage_path TEXT NOT NULL,
        media_type TEXT NOT NULL,
        original_name TEXT NOT NULL,
        size_bytes INTEGER NOT NULL,
        width INTEGER,
        height INTEGER,
        created_at TEXT NOT NULL,
        CHECK (kind IN ('image', 'document', 'text')),
        CHECK (storage_path <> ''),
        CHECK (media_type <> ''),
        CHECK (original_name <> ''),
        CHECK (size_bytes >= 0),
        FOREIGN KEY (project_id, run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_agent_message_attachments_message
        ON agent_message_attachments(message_id);
    CREATE INDEX IF NOT EXISTS idx_agent_message_attachments_run
        ON agent_message_attachments(project_id, run_id);
"#;

const BASELINE_SCHEMA_SQL: &str = r#"
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
        user_answer TEXT,
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
        CHECK (user_answer IS NULL OR user_answer <> ''),
        CHECK (status IN ('pending', 'approved', 'rejected')),
        CHECK (
            (status = 'pending' AND resolved_at IS NULL AND decision_note IS NULL AND user_answer IS NULL)
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

    CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_sessions_selected
        ON agent_sessions(project_id)
        WHERE selected = 1;
    CREATE INDEX IF NOT EXISTS idx_agent_sessions_project_status_updated
        ON agent_sessions(project_id, status, updated_at DESC);
    CREATE INDEX IF NOT EXISTS idx_agent_sessions_project_last_run
        ON agent_sessions(project_id, last_run_id)
        WHERE last_run_id IS NOT NULL;

    CREATE TABLE IF NOT EXISTS agent_definitions (
        definition_id TEXT PRIMARY KEY,
        current_version INTEGER NOT NULL CHECK (current_version > 0),
        display_name TEXT NOT NULL,
        short_label TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        scope TEXT NOT NULL,
        lifecycle_state TEXT NOT NULL,
        base_capability_profile TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        updated_at TEXT NOT NULL,
        CHECK (definition_id <> ''),
        CHECK (display_name <> ''),
        CHECK (short_label <> ''),
        CHECK (scope IN ('built_in', 'global_custom', 'project_custom')),
        CHECK (lifecycle_state IN ('draft', 'active', 'archived')),
        CHECK (base_capability_profile IN ('observe_only', 'engineering', 'debugging', 'agent_builder'))
    );

    CREATE TABLE IF NOT EXISTS agent_definition_versions (
        definition_id TEXT NOT NULL,
        version INTEGER NOT NULL CHECK (version > 0),
        snapshot_json TEXT NOT NULL,
        validation_report_json TEXT,
        created_at TEXT NOT NULL,
        PRIMARY KEY (definition_id, version),
        CHECK (definition_id <> ''),
        CHECK (snapshot_json <> '' AND json_valid(snapshot_json)),
        CHECK (validation_report_json IS NULL OR (validation_report_json <> '' AND json_valid(validation_report_json))),
        FOREIGN KEY (definition_id)
            REFERENCES agent_definitions(definition_id)
    );

    CREATE INDEX IF NOT EXISTS idx_agent_definitions_lifecycle_scope
        ON agent_definitions(lifecycle_state, scope, display_name);

    CREATE TRIGGER IF NOT EXISTS agent_definition_versions_immutable_update
    BEFORE UPDATE ON agent_definition_versions
    BEGIN
        SELECT RAISE(ABORT, 'agent definition versions are immutable');
    END;

    CREATE TRIGGER IF NOT EXISTS agent_definition_versions_immutable_delete
    BEFORE DELETE ON agent_definition_versions
    BEGIN
        SELECT RAISE(ABORT, 'agent definition versions are immutable');
    END;

    INSERT OR IGNORE INTO agent_definitions (
        definition_id,
        current_version,
        display_name,
        short_label,
        description,
        scope,
        lifecycle_state,
        base_capability_profile,
        updated_at
    )
    VALUES
        ('ask', 1, 'Ask', 'Ask', 'Answer questions about the project without mutating files, app state, processes, or external services.', 'built_in', 'active', 'observe_only', '2026-05-01T00:00:00Z'),
        ('engineer', 1, 'Engineer', 'Build', 'Implement repository changes with the existing software-building toolset and safety gates.', 'built_in', 'active', 'engineering', '2026-05-01T00:00:00Z'),
        ('debug', 1, 'Debug', 'Debug', 'Investigate failures with structured evidence, hypotheses, fixes, verification, and durable debugging memory.', 'built_in', 'active', 'debugging', '2026-05-01T00:00:00Z'),
        ('agent_create', 1, 'Agent Create', 'Create', 'Interview the user and draft high-quality custom agent definitions.', 'built_in', 'active', 'agent_builder', '2026-05-01T00:00:00Z');

    INSERT OR IGNORE INTO agent_definition_versions (
        definition_id,
        version,
        snapshot_json,
        validation_report_json,
        created_at
    )
    VALUES
        ('ask', 1, '{"id":"ask","version":1,"scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"observe_only","label":"Ask","shortLabel":"Ask"}', '{"status":"valid","source":"seed"}', '2026-05-01T00:00:00Z'),
        ('engineer', 1, '{"id":"engineer","version":1,"scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"engineering","label":"Engineer","shortLabel":"Build"}', '{"status":"valid","source":"seed"}', '2026-05-01T00:00:00Z'),
        ('debug', 1, '{"id":"debug","version":1,"scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"debugging","label":"Debug","shortLabel":"Debug"}', '{"status":"valid","source":"seed"}', '2026-05-01T00:00:00Z'),
        ('agent_create', 1, '{"id":"agent_create","version":1,"scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"agent_builder","label":"Agent Create","shortLabel":"Create"}', '{"status":"valid","source":"seed"}', '2026-05-01T00:00:00Z');

    CREATE TABLE IF NOT EXISTS runtime_runs (
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

    CREATE UNIQUE INDEX IF NOT EXISTS idx_runtime_runs_project_run
        ON runtime_runs(project_id, run_id);
    CREATE INDEX IF NOT EXISTS idx_runtime_runs_status_updated
        ON runtime_runs(project_id, agent_session_id, status, updated_at DESC);
    CREATE INDEX IF NOT EXISTS idx_runtime_runs_supervisor_kind
        ON runtime_runs(supervisor_kind, transport_liveness);
    CREATE INDEX IF NOT EXISTS idx_runtime_runs_provider_status_updated
        ON runtime_runs(provider_id, status, updated_at DESC);

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

    CREATE TABLE IF NOT EXISTS autonomous_runs (
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
            REFERENCES runtime_runs(project_id, run_id) ON DELETE CASCADE
    );

    CREATE UNIQUE INDEX IF NOT EXISTS idx_autonomous_runs_project_run
        ON autonomous_runs(project_id, run_id);
    CREATE INDEX IF NOT EXISTS idx_autonomous_runs_status_updated
        ON autonomous_runs(project_id, agent_session_id, status, updated_at DESC);
    CREATE INDEX IF NOT EXISTS idx_autonomous_runs_provider_status_updated
        ON autonomous_runs(provider_id, status, updated_at DESC);

    CREATE TABLE IF NOT EXISTS agent_runs (
        runtime_agent_id TEXT NOT NULL,
        agent_definition_id TEXT NOT NULL,
        agent_definition_version INTEGER NOT NULL,
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
        CHECK (runtime_agent_id <> ''),
        CHECK (agent_definition_id <> ''),
        CHECK (agent_definition_version > 0),
        CHECK (run_id <> ''),
        CHECK (provider_id <> ''),
        CHECK (model_id <> ''),
        CHECK (status IN ('starting', 'running', 'paused', 'cancelling', 'cancelled', 'handed_off', 'completed', 'failed')),
        CHECK (prompt <> ''),
        CHECK (system_prompt <> ''),
        CHECK (
            (last_error_code IS NULL AND last_error_message IS NULL)
            OR (last_error_code IS NOT NULL AND last_error_message IS NOT NULL)
        ),
        FOREIGN KEY (project_id, agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
        FOREIGN KEY (agent_definition_id, agent_definition_version)
            REFERENCES agent_definition_versions(definition_id, version)
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
            'tool_registry_snapshot',
            'policy_decision',
            'state_transition',
            'plan_updated',
            'verification_gate',
            'action_required',
            'run_paused',
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
        CHECK (checkpoint_kind IN ('preflight', 'message', 'tool', 'plan', 'validation', 'verification', 'completion', 'failure', 'recovery')),
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
        agent_definition_id TEXT NOT NULL,
        agent_definition_version INTEGER NOT NULL,
        provider_id TEXT NOT NULL,
        model_id TEXT NOT NULL,
        input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (input_tokens >= 0),
        output_tokens INTEGER NOT NULL DEFAULT 0 CHECK (output_tokens >= 0),
        total_tokens INTEGER NOT NULL DEFAULT 0 CHECK (total_tokens >= 0),
        estimated_cost_micros INTEGER NOT NULL DEFAULT 0 CHECK (estimated_cost_micros >= 0),
        updated_at TEXT NOT NULL,
        cache_read_tokens INTEGER NOT NULL DEFAULT 0 CHECK (cache_read_tokens >= 0),
        cache_creation_tokens INTEGER NOT NULL DEFAULT 0 CHECK (cache_creation_tokens >= 0),
        PRIMARY KEY (project_id, run_id),
        CHECK (agent_definition_id <> ''),
        CHECK (agent_definition_version > 0),
        CHECK (provider_id <> ''),
        CHECK (model_id <> ''),
        FOREIGN KEY (project_id, run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE,
        FOREIGN KEY (agent_definition_id, agent_definition_version)
            REFERENCES agent_definition_versions(definition_id, version)
    );

    CREATE INDEX IF NOT EXISTS idx_agent_usage_project_model
        ON agent_usage(project_id, provider_id, model_id);

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
                'message', 'The source run for this branch was deleted. Xero preserved the branch replay copy and cleared the source reference.'
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
                'message', 'The source session for this branch was deleted. Xero preserved the branch replay copy and cleared the source reference.'
            )
        WHERE project_id = old.project_id
          AND source_agent_session_id = old.agent_session_id;
    END;

    CREATE TABLE IF NOT EXISTS agent_context_policy_settings (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        scope_kind TEXT NOT NULL,
        agent_session_id TEXT,
        auto_compact_enabled INTEGER NOT NULL DEFAULT 1 CHECK (auto_compact_enabled IN (0, 1)),
        auto_handoff_enabled INTEGER NOT NULL DEFAULT 1 CHECK (auto_handoff_enabled IN (0, 1)),
        compact_threshold_percent INTEGER NOT NULL DEFAULT 75 CHECK (compact_threshold_percent BETWEEN 1 AND 100),
        handoff_threshold_percent INTEGER NOT NULL DEFAULT 90 CHECK (handoff_threshold_percent BETWEEN 1 AND 100),
        raw_tail_message_count INTEGER NOT NULL DEFAULT 8 CHECK (raw_tail_message_count >= 0),
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        updated_at TEXT NOT NULL,
        CHECK (scope_kind IN ('project', 'session')),
        CHECK (
            (scope_kind = 'project' AND agent_session_id IS NULL)
            OR (scope_kind = 'session' AND agent_session_id IS NOT NULL AND agent_session_id <> '')
        ),
        CHECK (compact_threshold_percent < handoff_threshold_percent),
        FOREIGN KEY (project_id, agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE
    );

    CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_context_policy_settings_project
        ON agent_context_policy_settings(project_id)
        WHERE scope_kind = 'project' AND agent_session_id IS NULL;
    CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_context_policy_settings_session
        ON agent_context_policy_settings(project_id, agent_session_id)
        WHERE scope_kind = 'session';

    CREATE TABLE IF NOT EXISTS agent_context_manifests (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        manifest_id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        agent_session_id TEXT NOT NULL,
        run_id TEXT,
        runtime_agent_id TEXT NOT NULL,
        agent_definition_id TEXT NOT NULL,
        agent_definition_version INTEGER NOT NULL,
        provider_id TEXT,
        model_id TEXT,
        request_kind TEXT NOT NULL,
        policy_action TEXT NOT NULL,
        policy_reason_code TEXT NOT NULL,
        budget_tokens INTEGER CHECK (budget_tokens IS NULL OR budget_tokens >= 0),
        estimated_tokens INTEGER NOT NULL DEFAULT 0 CHECK (estimated_tokens >= 0),
        pressure TEXT NOT NULL,
        context_hash TEXT NOT NULL,
        included_contributors_json TEXT NOT NULL,
        excluded_contributors_json TEXT NOT NULL,
        retrieval_query_ids_json TEXT NOT NULL DEFAULT '[]',
        retrieval_result_ids_json TEXT NOT NULL DEFAULT '[]',
        compaction_id TEXT,
        handoff_id TEXT,
        redaction_state TEXT NOT NULL DEFAULT 'clean',
        manifest_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        CHECK (manifest_id <> ''),
        CHECK (agent_session_id <> ''),
        CHECK (run_id IS NULL OR run_id <> ''),
        CHECK (runtime_agent_id <> ''),
        CHECK (agent_definition_id <> ''),
        CHECK (agent_definition_version > 0),
        CHECK (provider_id IS NULL OR provider_id <> ''),
        CHECK (model_id IS NULL OR model_id <> ''),
        CHECK (request_kind IN ('provider_turn', 'handoff_source', 'diagnostic', 'test')),
        CHECK (policy_action IN ('continue_now', 'compact_now', 'recompact_now', 'handoff_now', 'blocked')),
        CHECK (policy_reason_code <> ''),
        CHECK (pressure IN ('unknown', 'low', 'medium', 'high', 'over')),
        CHECK (length(context_hash) = 64),
        CHECK (context_hash NOT GLOB '*[^0-9a-f]*'),
        CHECK (included_contributors_json <> '' AND json_valid(included_contributors_json)),
        CHECK (excluded_contributors_json <> '' AND json_valid(excluded_contributors_json)),
        CHECK (retrieval_query_ids_json <> '' AND json_valid(retrieval_query_ids_json)),
        CHECK (retrieval_result_ids_json <> '' AND json_valid(retrieval_result_ids_json)),
        CHECK (compaction_id IS NULL OR compaction_id <> ''),
        CHECK (handoff_id IS NULL OR handoff_id <> ''),
        CHECK (redaction_state IN ('clean', 'redacted', 'blocked')),
        CHECK (manifest_json <> '' AND json_valid(manifest_json)),
        UNIQUE (project_id, manifest_id),
        FOREIGN KEY (project_id, agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE,
        FOREIGN KEY (agent_definition_id, agent_definition_version)
            REFERENCES agent_definition_versions(definition_id, version)
    );

    CREATE INDEX IF NOT EXISTS idx_agent_context_manifests_run_created
        ON agent_context_manifests(project_id, run_id, created_at DESC, id DESC);
    CREATE INDEX IF NOT EXISTS idx_agent_context_manifests_session_created
        ON agent_context_manifests(project_id, agent_session_id, created_at DESC, id DESC);
    CREATE INDEX IF NOT EXISTS idx_agent_context_manifests_policy
        ON agent_context_manifests(project_id, policy_action, created_at DESC);

    CREATE TABLE IF NOT EXISTS agent_handoff_lineage (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        handoff_id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        source_agent_session_id TEXT NOT NULL,
        source_run_id TEXT NOT NULL,
        source_runtime_agent_id TEXT NOT NULL,
        source_agent_definition_id TEXT NOT NULL,
        source_agent_definition_version INTEGER NOT NULL,
        target_agent_session_id TEXT,
        target_run_id TEXT,
        target_runtime_agent_id TEXT NOT NULL,
        target_agent_definition_id TEXT NOT NULL,
        target_agent_definition_version INTEGER NOT NULL,
        provider_id TEXT NOT NULL,
        model_id TEXT NOT NULL,
        source_context_hash TEXT NOT NULL,
        status TEXT NOT NULL,
        idempotency_key TEXT NOT NULL,
        handoff_record_id TEXT,
        bundle_json TEXT NOT NULL,
        diagnostic_json TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        completed_at TEXT,
        CHECK (handoff_id <> ''),
        CHECK (source_agent_session_id <> ''),
        CHECK (source_run_id <> ''),
        CHECK (source_runtime_agent_id <> ''),
        CHECK (source_agent_definition_id <> ''),
        CHECK (source_agent_definition_version > 0),
        CHECK (target_agent_session_id IS NULL OR target_agent_session_id <> ''),
        CHECK (target_run_id IS NULL OR target_run_id <> ''),
        CHECK (target_runtime_agent_id <> ''),
        CHECK (target_agent_definition_id <> ''),
        CHECK (target_agent_definition_version > 0),
        CHECK (source_agent_definition_id = target_agent_definition_id),
        CHECK (source_agent_definition_version = target_agent_definition_version),
        CHECK (provider_id <> ''),
        CHECK (model_id <> ''),
        CHECK (length(source_context_hash) = 64),
        CHECK (source_context_hash NOT GLOB '*[^0-9a-f]*'),
        CHECK (status IN ('pending', 'recorded', 'target_created', 'completed', 'failed')),
        CHECK (idempotency_key <> ''),
        CHECK (handoff_record_id IS NULL OR handoff_record_id <> ''),
        CHECK (bundle_json <> '' AND json_valid(bundle_json)),
        CHECK (diagnostic_json IS NULL OR (diagnostic_json <> '' AND json_valid(diagnostic_json))),
        CHECK (completed_at IS NULL OR completed_at <> ''),
        UNIQUE (project_id, handoff_id),
        UNIQUE (project_id, idempotency_key),
        FOREIGN KEY (project_id, source_agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, source_run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE,
        FOREIGN KEY (source_agent_definition_id, source_agent_definition_version)
            REFERENCES agent_definition_versions(definition_id, version),
        FOREIGN KEY (target_agent_definition_id, target_agent_definition_version)
            REFERENCES agent_definition_versions(definition_id, version)
    );

    CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_handoff_lineage_one_pending_source
        ON agent_handoff_lineage(project_id, source_run_id)
        WHERE status IN ('pending', 'recorded');
    CREATE INDEX IF NOT EXISTS idx_agent_handoff_lineage_status_updated
        ON agent_handoff_lineage(project_id, status, updated_at DESC, id DESC);
    CREATE INDEX IF NOT EXISTS idx_agent_handoff_lineage_target
        ON agent_handoff_lineage(project_id, target_run_id)
        WHERE target_run_id IS NOT NULL;

    CREATE TABLE IF NOT EXISTS agent_retrieval_queries (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        query_id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        agent_session_id TEXT,
        run_id TEXT,
        runtime_agent_id TEXT NOT NULL,
        agent_definition_id TEXT NOT NULL,
        agent_definition_version INTEGER NOT NULL,
        query_text TEXT NOT NULL,
        query_hash TEXT NOT NULL,
        search_scope TEXT NOT NULL,
        filters_json TEXT NOT NULL DEFAULT '{}',
        limit_count INTEGER NOT NULL CHECK (limit_count > 0),
        status TEXT NOT NULL,
        diagnostic_json TEXT,
        created_at TEXT NOT NULL,
        completed_at TEXT,
        CHECK (query_id <> ''),
        CHECK (agent_session_id IS NULL OR agent_session_id <> ''),
        CHECK (run_id IS NULL OR run_id <> ''),
        CHECK (runtime_agent_id <> ''),
        CHECK (agent_definition_id <> ''),
        CHECK (agent_definition_version > 0),
        CHECK (query_text <> ''),
        CHECK (length(query_hash) = 64),
        CHECK (query_hash NOT GLOB '*[^0-9a-f]*'),
        CHECK (search_scope IN ('project_records', 'approved_memory', 'hybrid_context', 'handoffs')),
        CHECK (filters_json <> '' AND json_valid(filters_json)),
        CHECK (status IN ('started', 'succeeded', 'failed')),
        CHECK (diagnostic_json IS NULL OR (diagnostic_json <> '' AND json_valid(diagnostic_json))),
        CHECK (completed_at IS NULL OR completed_at <> ''),
        UNIQUE (project_id, query_id),
        FOREIGN KEY (project_id, agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE,
        FOREIGN KEY (agent_definition_id, agent_definition_version)
            REFERENCES agent_definition_versions(definition_id, version)
    );

    CREATE INDEX IF NOT EXISTS idx_agent_retrieval_queries_run_created
        ON agent_retrieval_queries(project_id, run_id, created_at DESC, id DESC);
    CREATE INDEX IF NOT EXISTS idx_agent_retrieval_queries_scope_status
        ON agent_retrieval_queries(project_id, search_scope, status, created_at DESC);

    CREATE TABLE IF NOT EXISTS agent_retrieval_results (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        project_id TEXT NOT NULL,
        query_id TEXT NOT NULL,
        result_id TEXT NOT NULL,
        source_kind TEXT NOT NULL,
        source_id TEXT NOT NULL,
        rank INTEGER NOT NULL CHECK (rank > 0),
        score REAL,
        snippet TEXT NOT NULL,
        redaction_state TEXT NOT NULL DEFAULT 'clean',
        metadata_json TEXT,
        created_at TEXT NOT NULL,
        CHECK (query_id <> ''),
        CHECK (result_id <> ''),
        CHECK (source_kind IN ('project_record', 'approved_memory', 'handoff', 'context_manifest')),
        CHECK (source_id <> ''),
        CHECK (score IS NULL OR score >= 0.0),
        CHECK (snippet <> ''),
        CHECK (redaction_state IN ('clean', 'redacted', 'blocked')),
        CHECK (metadata_json IS NULL OR (metadata_json <> '' AND json_valid(metadata_json))),
        UNIQUE (project_id, query_id, result_id),
        FOREIGN KEY (project_id, query_id)
            REFERENCES agent_retrieval_queries(project_id, query_id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_agent_retrieval_results_query_rank
        ON agent_retrieval_results(project_id, query_id, rank ASC, id ASC);
    CREATE INDEX IF NOT EXISTS idx_agent_retrieval_results_source
        ON agent_retrieval_results(project_id, source_kind, source_id);

    CREATE TABLE IF NOT EXISTS agent_embedding_backfill_jobs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        job_id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        source_kind TEXT NOT NULL,
        source_id TEXT NOT NULL,
        source_hash TEXT NOT NULL,
        embedding_model TEXT NOT NULL,
        embedding_dimension INTEGER NOT NULL CHECK (embedding_dimension > 0),
        embedding_version TEXT NOT NULL,
        status TEXT NOT NULL,
        attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
        diagnostic_json TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        completed_at TEXT,
        CHECK (job_id <> ''),
        CHECK (source_kind IN ('project_record', 'approved_memory')),
        CHECK (source_id <> ''),
        CHECK (length(source_hash) = 64),
        CHECK (source_hash NOT GLOB '*[^0-9a-f]*'),
        CHECK (embedding_model <> ''),
        CHECK (embedding_version <> ''),
        CHECK (status IN ('pending', 'running', 'succeeded', 'failed', 'skipped')),
        CHECK (diagnostic_json IS NULL OR (diagnostic_json <> '' AND json_valid(diagnostic_json))),
        CHECK (completed_at IS NULL OR completed_at <> ''),
        UNIQUE (project_id, job_id),
        UNIQUE (project_id, source_kind, source_id, embedding_model, embedding_version)
    );

    CREATE INDEX IF NOT EXISTS idx_agent_embedding_backfill_jobs_status
        ON agent_embedding_backfill_jobs(project_id, status, created_at ASC, id ASC);
    CREATE INDEX IF NOT EXISTS idx_agent_embedding_backfill_jobs_source
        ON agent_embedding_backfill_jobs(project_id, source_kind, source_id);

    CREATE TABLE IF NOT EXISTS meta (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        project_id TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        CHECK (project_id <> '')
    );
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};

    fn migrate_to_latest_in_memory() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;\nPRAGMA journal_mode = MEMORY;\nPRAGMA synchronous = NORMAL;",
            )
            .expect("apply pragmas");
        migrations()
            .to_latest(&mut connection)
            .expect("migrate to latest schema");
        connection
    }

    fn collect_strings(connection: &Connection, sql: &str) -> Vec<String> {
        let mut statement = connection.prepare(sql).expect("prepare schema query");
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query schema names");
        rows.collect::<Result<Vec<_>, _>>()
            .expect("collect schema names")
    }

    fn table_columns(connection: &Connection, table: &str) -> Vec<String> {
        let mut statement = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .expect("prepare PRAGMA table_info");
        let rows = statement
            .query_map([], |row| row.get::<_, String>(1))
            .expect("query PRAGMA table_info");
        rows.collect::<Result<Vec<_>, _>>()
            .expect("collect column names")
    }

    #[test]
    fn migrations_validate() {
        migrations()
            .validate()
            .expect("project migrations validate");
    }

    #[test]
    fn migrations_run_to_latest_on_a_fresh_in_memory_connection() {
        let _connection = migrate_to_latest_in_memory();
    }

    #[test]
    fn baseline_contains_current_project_tables() {
        let connection = migrate_to_latest_in_memory();
        let tables = collect_strings(
            &connection,
            r#"
            SELECT name
            FROM sqlite_master
            WHERE type = 'table'
              AND name NOT LIKE 'sqlite_%'
            ORDER BY name
            "#,
        );

        assert_eq!(
            tables,
            vec![
                "agent_action_requests",
                "agent_checkpoints",
                "agent_compactions",
                "agent_context_manifests",
                "agent_context_policy_settings",
                "agent_definition_versions",
                "agent_definitions",
                "agent_embedding_backfill_jobs",
                "agent_events",
                "agent_file_changes",
                "agent_handoff_lineage",
                "agent_message_attachments",
                "agent_messages",
                "agent_retrieval_queries",
                "agent_retrieval_results",
                "agent_runs",
                "agent_session_lineage",
                "agent_sessions",
                "agent_tool_calls",
                "agent_usage",
                "autonomous_runs",
                "installed_plugin_records",
                "installed_skill_records",
                "meta",
                "notification_dispatches",
                "notification_reply_claims",
                "notification_routes",
                "operator_approvals",
                "operator_resume_history",
                "operator_verification_records",
                "projects",
                "repositories",
                "runtime_run_checkpoints",
                "runtime_runs",
                "runtime_sessions",
            ],
            "fresh project databases should start from the current baseline schema"
        );
    }

    #[test]
    fn deprecated_workflow_and_autonomous_unit_tables_are_absent_from_baseline() {
        let connection = migrate_to_latest_in_memory();
        let tables = collect_strings(
            &connection,
            r#"
            SELECT name
            FROM sqlite_master
            WHERE type = 'table'
              AND name IN (
                'workflow_phases',
                'workflow_graph_nodes',
                'workflow_graph_edges',
                'workflow_gate_metadata',
                'workflow_transition_events',
                'workflow_handoff_packages',
                'autonomous_units',
                'autonomous_unit_attempts',
                'autonomous_unit_artifacts',
                'agent_memories'
              )
            "#,
        );
        assert!(
            tables.is_empty(),
            "deprecated workflow/autonomous-unit/memory tables should not exist in the fresh baseline: {tables:?}"
        );
    }

    #[test]
    fn phase1_continuity_schema_persists_manifest_and_rejects_cross_definition_handoff() {
        let connection = migrate_to_latest_in_memory();
        connection
            .execute(
                "INSERT INTO projects (id, name) VALUES (?1, ?2)",
                params!["project-1", "Project"],
            )
            .expect("seed project");
        connection
            .execute(
                r#"
                INSERT INTO agent_sessions (
                    project_id,
                    agent_session_id,
                    title,
                    status,
                    selected,
                    created_at,
                    updated_at
                )
                VALUES (?1, ?2, 'Main', 'active', 1, ?3, ?3)
                "#,
                params!["project-1", "agent-session-main", "2026-05-01T12:00:00Z"],
            )
            .expect("seed agent session");

        connection
            .execute(
                r#"
                INSERT INTO agent_context_policy_settings (
                    project_id,
                    scope_kind,
                    auto_compact_enabled,
                    auto_handoff_enabled,
                    compact_threshold_percent,
                    handoff_threshold_percent,
                    raw_tail_message_count,
                    updated_at
                )
                VALUES (?1, 'project', 1, 1, 75, 90, 8, ?2)
                "#,
                params!["project-1", "2026-05-01T12:01:00Z"],
            )
            .expect("insert context policy settings");

        connection
            .execute(
                r#"
                INSERT INTO agent_context_manifests (
                    manifest_id,
                    project_id,
                    agent_session_id,
                    runtime_agent_id,
                    agent_definition_id,
                    agent_definition_version,
                    request_kind,
                    policy_action,
                    policy_reason_code,
                    estimated_tokens,
                    pressure,
                    context_hash,
                    included_contributors_json,
                    excluded_contributors_json,
                    retrieval_query_ids_json,
                    retrieval_result_ids_json,
                    redaction_state,
                    manifest_json,
                    created_at
                )
                VALUES (?1, ?2, ?3, 'ask', 'ask', 1, 'test', 'continue_now', 'schema_test', 10, 'unknown', ?4, '[]', '[]', '[]', '[]', 'clean', ?5, ?6)
                "#,
                params![
                    "manifest-1",
                    "project-1",
                    "agent-session-main",
                    "a".repeat(64),
                    r#"{"kind":"pre_provider"}"#,
                    "2026-05-01T12:02:00Z",
                ],
            )
            .expect("manifest persists without run/provider/model");

        connection
            .execute(
                r#"
                INSERT INTO agent_runs (
                    runtime_agent_id,
                    agent_definition_id,
                    agent_definition_version,
                    project_id,
                    agent_session_id,
                    run_id,
                    provider_id,
                    model_id,
                    status,
                    prompt,
                    system_prompt,
                    started_at,
                    updated_at
                )
                VALUES ('ask', 'ask', 1, ?1, ?2, ?3, 'fake_provider', 'fake-model', 'running', 'prompt', 'system', ?4, ?4)
                "#,
                params![
                    "project-1",
                    "agent-session-main",
                    "run-1",
                    "2026-05-01T12:03:00Z"
                ],
            )
            .expect("seed agent run");

        let mismatch = connection
            .execute(
                r#"
                INSERT INTO agent_handoff_lineage (
                    handoff_id,
                    project_id,
                    source_agent_session_id,
                    source_run_id,
                    source_runtime_agent_id,
                    source_agent_definition_id,
                    source_agent_definition_version,
                    target_runtime_agent_id,
                    target_agent_definition_id,
                    target_agent_definition_version,
                    provider_id,
                    model_id,
                    source_context_hash,
                    status,
                    idempotency_key,
                    bundle_json,
                    created_at,
                    updated_at
                )
                VALUES ('handoff-bad', ?1, ?2, ?3, 'ask', 'ask', 1, 'ask', 'debug', 1, 'fake_provider', 'fake-model', ?4, 'pending', 'handoff-bad-key', ?5, ?6, ?6)
                "#,
                params![
                    "project-1",
                    "agent-session-main",
                    "run-1",
                    "b".repeat(64),
                    r#"{"targetRuntimeAgentId":"debug"}"#,
                    "2026-05-01T12:04:00Z"
                ],
            )
            .expect_err("schema rejects cross-definition handoff");
        assert!(mismatch.to_string().contains("CHECK constraint failed"));

        connection
            .execute(
                r#"
                INSERT INTO agent_handoff_lineage (
                    handoff_id,
                    project_id,
                    source_agent_session_id,
                    source_run_id,
                    source_runtime_agent_id,
                    source_agent_definition_id,
                    source_agent_definition_version,
                    target_runtime_agent_id,
                    target_agent_definition_id,
                    target_agent_definition_version,
                    provider_id,
                    model_id,
                    source_context_hash,
                    status,
                    idempotency_key,
                    bundle_json,
                    created_at,
                    updated_at
                )
                VALUES ('handoff-good', ?1, ?2, ?3, 'ask', 'ask', 1, 'ask', 'ask', 1, 'fake_provider', 'fake-model', ?4, 'pending', 'handoff-good-key', ?5, ?6, ?6)
                "#,
                params![
                    "project-1",
                    "agent-session-main",
                    "run-1",
                    "b".repeat(64),
                    r#"{"targetRuntimeAgentId":"ask"}"#,
                    "2026-05-01T12:04:00Z"
                ],
            )
            .expect("schema accepts same-type handoff");

        connection
            .execute(
                r#"
                INSERT INTO agent_retrieval_queries (
                    query_id,
                    project_id,
                    agent_session_id,
                    runtime_agent_id,
                    agent_definition_id,
                    agent_definition_version,
                    query_text,
                    query_hash,
                    search_scope,
                    filters_json,
                    limit_count,
                    status,
                    created_at
                )
                VALUES ('query-1', ?1, ?2, 'ask', 'ask', 1, 'handoffs', ?3, 'handoffs', '{}', 5, 'succeeded', ?4)
                "#,
                params![
                    "project-1",
                    "agent-session-main",
                    "c".repeat(64),
                    "2026-05-01T12:05:00Z"
                ],
            )
            .expect("insert retrieval query log");
        connection
            .execute(
                r#"
                INSERT INTO agent_retrieval_results (
                    project_id,
                    query_id,
                    result_id,
                    source_kind,
                    source_id,
                    rank,
                    snippet,
                    redaction_state,
                    created_at
                )
                VALUES (?1, 'query-1', 'result-1', 'handoff', 'handoff-good', 1, 'Same-type handoff.', 'clean', ?2)
                "#,
                params!["project-1", "2026-05-01T12:05:01Z"],
            )
            .expect("insert retrieval result log");
    }

    #[test]
    fn operator_approvals_uses_decoupled_human_in_the_loop_schema() {
        let connection = migrate_to_latest_in_memory();
        let columns = table_columns(&connection, "operator_approvals");
        assert_eq!(
            columns,
            vec![
                "project_id",
                "action_id",
                "session_id",
                "flow_id",
                "action_type",
                "title",
                "detail",
                "user_answer",
                "status",
                "decision_note",
                "created_at",
                "updated_at",
                "resolved_at",
            ],
            "operator approvals should keep only the live approval-loop columns"
        );

        let indexes = collect_strings(
            &connection,
            r#"
            SELECT name
            FROM sqlite_master
            WHERE type = 'index'
              AND tbl_name = 'operator_approvals'
              AND name LIKE 'idx_operator_approvals_%'
            ORDER BY name
            "#,
        );
        assert_eq!(
            indexes,
            vec!["idx_operator_approvals_project_status_updated"],
            "operator approvals should not carry workflow-era indexes"
        );
    }

    #[test]
    fn migrations_are_idempotent() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("apply pragmas");
        migrations()
            .to_latest(&mut connection)
            .expect("first migration");
        migrations()
            .to_latest(&mut connection)
            .expect("second migration is a no-op");
    }
}
