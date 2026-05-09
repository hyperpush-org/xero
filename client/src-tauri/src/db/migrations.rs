use std::sync::LazyLock;

use rusqlite::Transaction;
use rusqlite_migration::{Migrations, M};

pub fn migrations() -> &'static Migrations<'static> {
    static MIGRATIONS: LazyLock<Migrations<'static>> = LazyLock::new(|| {
        Migrations::new(vec![
            M::up(BASELINE_SCHEMA_SQL),
            M::up(MIGRATION_001_AGENT_MESSAGE_ATTACHMENTS_SQL),
            M::up(MIGRATION_002_WORKSPACE_INDEX_SQL),
            M::up(MIGRATION_003_AGENT_COORDINATION_SQL),
            M::up(MIGRATION_004_AGENT_MAILBOX_SQL),
            M::up(MIGRATION_005_AGENT_PLAN_PACKS_SQL),
            M::up_with_hook("", migrate_agent_trace_columns),
            M::up_with_hook("", migrate_environment_lifecycle_schema),
            M::up_with_hook("", migrate_agent_events_current_event_kinds),
            M::up_with_hook("", migrate_project_origin_column),
            M::up(MIGRATION_010_CODE_ROLLBACK_STORAGE_SQL),
            M::up(MIGRATION_011_CODE_HISTORY_WORKSPACE_HEAD_SQL),
            M::up(MIGRATION_012_CODE_HISTORY_COMMIT_PATCHSET_SQL),
            M::up(MIGRATION_013_CODE_HISTORY_OPERATIONS_SQL),
            M::up(MIGRATION_014_AGENT_RESERVATION_INVALIDATIONS_SQL),
            M::up_with_hook("", migrate_agent_trace_columns_repair),
            M::up(MIGRATION_015_CROSS_STORE_OUTBOX_SQL),
            M::up(MIGRATION_016_PROJECT_STORAGE_MAINTENANCE_SQL),
            M::up(MIGRATION_017_AGENT_AUDIT_AND_REVOCATION_SQL),
            M::up_with_hook("", migrate_agent_messages_provider_metadata_json),
            M::up(MIGRATION_018_AGENT_SUBAGENT_TASKS_SQL),
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

const MIGRATION_002_WORKSPACE_INDEX_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS workspace_index_metadata (
        project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
        status TEXT NOT NULL CHECK (status IN ('empty', 'indexing', 'ready', 'stale', 'failed')),
        index_version INTEGER NOT NULL CHECK (index_version > 0),
        root_path TEXT NOT NULL CHECK (root_path <> ''),
        storage_path TEXT NOT NULL CHECK (storage_path <> ''),
        head_sha TEXT,
        worktree_fingerprint TEXT,
        total_files INTEGER NOT NULL DEFAULT 0 CHECK (total_files >= 0),
        indexed_files INTEGER NOT NULL DEFAULT 0 CHECK (indexed_files >= 0),
        skipped_files INTEGER NOT NULL DEFAULT 0 CHECK (skipped_files >= 0),
        stale_files INTEGER NOT NULL DEFAULT 0 CHECK (stale_files >= 0),
        symbol_count INTEGER NOT NULL DEFAULT 0 CHECK (symbol_count >= 0),
        indexed_bytes INTEGER NOT NULL DEFAULT 0 CHECK (indexed_bytes >= 0),
        coverage_percent REAL NOT NULL DEFAULT 0 CHECK (coverage_percent >= 0 AND coverage_percent <= 100),
        diagnostics_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(diagnostics_json)),
        last_error_code TEXT,
        last_error_message TEXT,
        started_at TEXT,
        completed_at TEXT,
        updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS workspace_index_files (
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        path TEXT NOT NULL CHECK (path <> ''),
        language TEXT NOT NULL CHECK (language <> ''),
        content_hash TEXT NOT NULL CHECK (content_hash <> ''),
        modified_at TEXT NOT NULL CHECK (modified_at <> ''),
        byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
        summary TEXT NOT NULL,
        snippet TEXT NOT NULL,
        symbols_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(symbols_json)),
        imports_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(imports_json)),
        tests_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(tests_json)),
        routes_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(routes_json)),
        commands_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(commands_json)),
        diffs_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(diffs_json)),
        failures_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(failures_json)),
        embedding_json TEXT NOT NULL CHECK (embedding_json <> '' AND json_valid(embedding_json)),
        embedding_model TEXT NOT NULL CHECK (embedding_model <> ''),
        embedding_version TEXT NOT NULL CHECK (embedding_version <> ''),
        indexed_at TEXT NOT NULL,
        PRIMARY KEY (project_id, path)
    );

    CREATE INDEX IF NOT EXISTS idx_workspace_index_files_project_language
        ON workspace_index_files(project_id, language, path);
    CREATE INDEX IF NOT EXISTS idx_workspace_index_files_project_hash
        ON workspace_index_files(project_id, content_hash);
"#;

const MIGRATION_003_AGENT_COORDINATION_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS agent_coordination_presence (
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        agent_session_id TEXT NOT NULL,
        run_id TEXT NOT NULL,
        trace_id TEXT NOT NULL,
        lineage_kind TEXT NOT NULL,
        parent_run_id TEXT,
        parent_subagent_id TEXT,
        role TEXT,
        pane_id TEXT,
        status TEXT NOT NULL,
        current_phase TEXT NOT NULL,
        activity_summary TEXT NOT NULL,
        last_event_id INTEGER,
        last_event_kind TEXT,
        started_at TEXT NOT NULL,
        last_heartbeat_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        expires_at TEXT NOT NULL,
        PRIMARY KEY (project_id, run_id),
        CHECK (agent_session_id <> ''),
        CHECK (run_id <> ''),
        CHECK (length(trace_id) = 32 AND trace_id NOT GLOB '*[^0-9a-f]*'),
        CHECK (lineage_kind IN ('top_level', 'subagent_child')),
        CHECK (parent_run_id IS NULL OR parent_run_id <> ''),
        CHECK (parent_subagent_id IS NULL OR parent_subagent_id <> ''),
        CHECK (role IS NULL OR role <> ''),
        CHECK (pane_id IS NULL OR pane_id <> ''),
        CHECK (status IN ('starting', 'running', 'paused', 'cancelling', 'cancelled', 'handed_off', 'completed', 'failed')),
        CHECK (current_phase <> ''),
        CHECK (activity_summary <> ''),
        CHECK (last_event_kind IS NULL OR last_event_kind <> ''),
        FOREIGN KEY (project_id, run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_agent_coordination_presence_project_expires
        ON agent_coordination_presence(project_id, expires_at, updated_at DESC);
    CREATE INDEX IF NOT EXISTS idx_agent_coordination_presence_session
        ON agent_coordination_presence(project_id, agent_session_id, updated_at DESC);

    CREATE TABLE IF NOT EXISTS agent_coordination_events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        run_id TEXT NOT NULL,
        trace_id TEXT NOT NULL,
        event_kind TEXT NOT NULL,
        summary TEXT NOT NULL,
        payload_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        expires_at TEXT NOT NULL,
        CHECK (run_id <> ''),
        CHECK (length(trace_id) = 32 AND trace_id NOT GLOB '*[^0-9a-f]*'),
        CHECK (event_kind <> ''),
        CHECK (summary <> ''),
        CHECK (payload_json <> '' AND json_valid(payload_json)),
        FOREIGN KEY (project_id, run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_agent_coordination_events_project_created
        ON agent_coordination_events(project_id, created_at DESC, id DESC);
    CREATE INDEX IF NOT EXISTS idx_agent_coordination_events_run_created
        ON agent_coordination_events(project_id, run_id, created_at DESC, id DESC);

    CREATE TABLE IF NOT EXISTS agent_file_reservations (
        reservation_id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        path TEXT NOT NULL,
        path_kind TEXT NOT NULL,
        operation TEXT NOT NULL,
        owner_agent_session_id TEXT NOT NULL,
        owner_run_id TEXT NOT NULL,
        owner_child_run_id TEXT,
        owner_role TEXT,
        owner_pane_id TEXT,
        owner_trace_id TEXT NOT NULL,
        note TEXT,
        override_reason TEXT,
        claimed_at TEXT NOT NULL,
        last_heartbeat_at TEXT NOT NULL,
        expires_at TEXT NOT NULL,
        released_at TEXT,
        release_reason TEXT,
        PRIMARY KEY (project_id, reservation_id),
        CHECK (reservation_id <> ''),
        CHECK (path <> ''),
        CHECK (path_kind IN ('path', 'prefix')),
        CHECK (operation IN ('observing', 'editing', 'refactoring', 'testing', 'verifying', 'writing')),
        CHECK (owner_agent_session_id <> ''),
        CHECK (owner_run_id <> ''),
        CHECK (owner_child_run_id IS NULL OR owner_child_run_id <> ''),
        CHECK (owner_role IS NULL OR owner_role <> ''),
        CHECK (owner_pane_id IS NULL OR owner_pane_id <> ''),
        CHECK (length(owner_trace_id) = 32 AND owner_trace_id NOT GLOB '*[^0-9a-f]*'),
        CHECK (note IS NULL OR note <> ''),
        CHECK (override_reason IS NULL OR override_reason <> ''),
        CHECK (
            (released_at IS NULL AND release_reason IS NULL)
            OR (released_at IS NOT NULL AND release_reason IS NOT NULL AND release_reason <> '')
        ),
        FOREIGN KEY (project_id, owner_agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, owner_run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, owner_child_run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_agent_file_reservations_project_active
        ON agent_file_reservations(project_id, released_at, expires_at, path);
    CREATE INDEX IF NOT EXISTS idx_agent_file_reservations_owner
        ON agent_file_reservations(project_id, owner_run_id, owner_child_run_id, released_at);
"#;

const MIGRATION_004_AGENT_MAILBOX_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS agent_mailbox_items (
        item_id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        item_type TEXT NOT NULL,
        parent_item_id TEXT,
        sender_agent_session_id TEXT NOT NULL,
        sender_run_id TEXT NOT NULL,
        sender_parent_run_id TEXT,
        sender_child_run_id TEXT,
        sender_role TEXT,
        sender_trace_id TEXT NOT NULL,
        target_agent_session_id TEXT,
        target_run_id TEXT,
        target_role TEXT,
        title TEXT NOT NULL,
        body TEXT NOT NULL,
        related_paths_json TEXT NOT NULL DEFAULT '[]',
        priority TEXT NOT NULL DEFAULT 'normal',
        status TEXT NOT NULL DEFAULT 'open',
        created_at TEXT NOT NULL,
        expires_at TEXT NOT NULL,
        resolved_at TEXT,
        resolved_by_run_id TEXT,
        resolve_reason TEXT,
        promoted_record_id TEXT,
        promoted_at TEXT,
        PRIMARY KEY (project_id, item_id),
        CHECK (item_id <> ''),
        CHECK (item_type IN (
            'heads_up',
            'question',
            'answer',
            'blocker',
            'file_ownership_note',
            'finding_in_progress',
            'verification_note',
            'handoff_lite_summary',
            'history_rewrite_notice',
            'undo_conflict_notice',
            'workspace_epoch_advanced',
            'reservation_invalidated'
        )),
        CHECK (parent_item_id IS NULL OR parent_item_id <> ''),
        CHECK (sender_agent_session_id <> ''),
        CHECK (sender_run_id <> ''),
        CHECK (sender_parent_run_id IS NULL OR sender_parent_run_id <> ''),
        CHECK (sender_child_run_id IS NULL OR sender_child_run_id <> ''),
        CHECK (sender_role IS NULL OR sender_role <> ''),
        CHECK (length(sender_trace_id) = 32 AND sender_trace_id NOT GLOB '*[^0-9a-f]*'),
        CHECK (target_agent_session_id IS NULL OR target_agent_session_id <> ''),
        CHECK (target_run_id IS NULL OR target_run_id <> ''),
        CHECK (target_role IS NULL OR target_role <> ''),
        CHECK (title <> ''),
        CHECK (body <> ''),
        CHECK (related_paths_json <> '' AND json_valid(related_paths_json)),
        CHECK (priority IN ('low', 'normal', 'high', 'urgent')),
        CHECK (status IN ('open', 'resolved', 'promoted')),
        CHECK (
            (resolved_at IS NULL AND resolved_by_run_id IS NULL AND resolve_reason IS NULL)
            OR (resolved_at IS NOT NULL AND resolved_by_run_id IS NOT NULL AND resolve_reason IS NOT NULL AND resolve_reason <> '')
        ),
        CHECK (
            (promoted_record_id IS NULL AND promoted_at IS NULL)
            OR (promoted_record_id IS NOT NULL AND promoted_record_id <> '' AND promoted_at IS NOT NULL)
        ),
        FOREIGN KEY (project_id, parent_item_id)
            REFERENCES agent_mailbox_items(project_id, item_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, sender_agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, sender_run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, sender_parent_run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, sender_child_run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, target_agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, target_run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_agent_mailbox_items_project_delivery
        ON agent_mailbox_items(project_id, status, expires_at, created_at DESC);
    CREATE INDEX IF NOT EXISTS idx_agent_mailbox_items_sender
        ON agent_mailbox_items(project_id, sender_run_id, created_at DESC);
    CREATE INDEX IF NOT EXISTS idx_agent_mailbox_items_target_run
        ON agent_mailbox_items(project_id, target_run_id, status, expires_at);
    CREATE INDEX IF NOT EXISTS idx_agent_mailbox_items_target_session
        ON agent_mailbox_items(project_id, target_agent_session_id, status, expires_at);
    CREATE INDEX IF NOT EXISTS idx_agent_mailbox_items_parent
        ON agent_mailbox_items(project_id, parent_item_id, created_at ASC);

    CREATE TABLE IF NOT EXISTS agent_mailbox_acknowledgements (
        project_id TEXT NOT NULL,
        item_id TEXT NOT NULL,
        agent_session_id TEXT NOT NULL,
        run_id TEXT NOT NULL,
        acknowledged_at TEXT NOT NULL,
        PRIMARY KEY (project_id, item_id, run_id),
        CHECK (item_id <> ''),
        CHECK (agent_session_id <> ''),
        CHECK (run_id <> ''),
        CHECK (acknowledged_at <> ''),
        FOREIGN KEY (project_id, item_id)
            REFERENCES agent_mailbox_items(project_id, item_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_agent_mailbox_acknowledgements_run
        ON agent_mailbox_acknowledgements(project_id, run_id, acknowledged_at DESC);
"#;

const MIGRATION_014_AGENT_RESERVATION_INVALIDATIONS_SQL: &str = r#"
    ALTER TABLE agent_file_reservations ADD COLUMN invalidated_at TEXT;
    ALTER TABLE agent_file_reservations ADD COLUMN invalidation_reason TEXT;
    ALTER TABLE agent_file_reservations ADD COLUMN invalidating_history_operation_id TEXT;

    CREATE INDEX IF NOT EXISTS idx_agent_file_reservations_invalidated
        ON agent_file_reservations(project_id, invalidated_at, invalidating_history_operation_id);
"#;

const MIGRATION_015_CROSS_STORE_OUTBOX_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS cross_store_outbox (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        operation_id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        store_kind TEXT NOT NULL,
        entity_kind TEXT NOT NULL,
        entity_id TEXT NOT NULL,
        operation TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'pending',
        payload_json TEXT NOT NULL,
        diagnostic_json TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        completed_at TEXT,
        CHECK (operation_id <> ''),
        CHECK (store_kind <> ''),
        CHECK (entity_kind <> ''),
        CHECK (entity_id <> ''),
        CHECK (operation <> ''),
        CHECK (status IN ('pending', 'applied', 'failed', 'reconciled')),
        CHECK (payload_json <> '' AND json_valid(payload_json)),
        CHECK (diagnostic_json IS NULL OR (diagnostic_json <> '' AND json_valid(diagnostic_json))),
        CHECK (created_at <> ''),
        CHECK (updated_at <> ''),
        CHECK (completed_at IS NULL OR completed_at <> ''),
        UNIQUE (project_id, operation_id)
    );

    CREATE INDEX IF NOT EXISTS idx_cross_store_outbox_status
        ON cross_store_outbox(project_id, status, created_at ASC, id ASC);
    CREATE INDEX IF NOT EXISTS idx_cross_store_outbox_entity
        ON cross_store_outbox(project_id, entity_kind, entity_id, status);
"#;

const MIGRATION_016_PROJECT_STORAGE_MAINTENANCE_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS project_storage_maintenance_runs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        run_id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        maintenance_kind TEXT NOT NULL,
        status TEXT NOT NULL,
        diagnostic_json TEXT,
        started_at TEXT NOT NULL,
        completed_at TEXT,
        CHECK (run_id <> ''),
        CHECK (maintenance_kind <> ''),
        CHECK (status IN ('running', 'succeeded', 'failed')),
        CHECK (diagnostic_json IS NULL OR (diagnostic_json <> '' AND json_valid(diagnostic_json))),
        CHECK (started_at <> ''),
        CHECK (completed_at IS NULL OR completed_at <> ''),
        UNIQUE (project_id, run_id)
    );

    CREATE INDEX IF NOT EXISTS idx_project_storage_maintenance_runs_latest
        ON project_storage_maintenance_runs(project_id, status, completed_at DESC, id DESC);
"#;

const MIGRATION_017_AGENT_AUDIT_AND_REVOCATION_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS agent_runtime_audit_events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        audit_id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        actor_kind TEXT NOT NULL,
        actor_id TEXT,
        action_kind TEXT NOT NULL,
        subject_kind TEXT NOT NULL,
        subject_id TEXT NOT NULL,
        run_id TEXT,
        agent_definition_id TEXT,
        agent_definition_version INTEGER,
        risk_class TEXT,
        approval_action_id TEXT,
        payload_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload_json)),
        created_at TEXT NOT NULL,
        CHECK (audit_id <> ''),
        CHECK (actor_kind IN ('system', 'user', 'agent', 'runtime')),
        CHECK (action_kind <> ''),
        CHECK (subject_kind <> ''),
        CHECK (subject_id <> ''),
        CHECK (run_id IS NULL OR run_id <> ''),
        CHECK (agent_definition_id IS NULL OR agent_definition_id <> ''),
        CHECK (agent_definition_version IS NULL OR agent_definition_version > 0),
        CHECK (risk_class IS NULL OR risk_class <> ''),
        CHECK (approval_action_id IS NULL OR approval_action_id <> ''),
        UNIQUE (project_id, audit_id)
    );

    CREATE INDEX IF NOT EXISTS idx_agent_runtime_audit_events_subject
        ON agent_runtime_audit_events(project_id, subject_kind, subject_id, created_at DESC, id DESC);
    CREATE INDEX IF NOT EXISTS idx_agent_runtime_audit_events_run
        ON agent_runtime_audit_events(project_id, run_id, created_at DESC, id DESC)
        WHERE run_id IS NOT NULL;

    CREATE TABLE IF NOT EXISTS agent_capability_revocations (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        revocation_id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        subject_kind TEXT NOT NULL,
        subject_id TEXT NOT NULL,
        scope_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(scope_json)),
        reason TEXT NOT NULL,
        created_by TEXT NOT NULL,
        status TEXT NOT NULL,
        created_at TEXT NOT NULL,
        cleared_at TEXT,
        CHECK (revocation_id <> ''),
        CHECK (subject_kind IN ('custom_agent', 'tool_pack', 'external_integration', 'browser_control', 'destructive_write')),
        CHECK (subject_id <> ''),
        CHECK (reason <> ''),
        CHECK (created_by <> ''),
        CHECK (status IN ('active', 'cleared')),
        CHECK ((status = 'active' AND cleared_at IS NULL) OR (status = 'cleared' AND cleared_at IS NOT NULL)),
        UNIQUE (project_id, revocation_id)
    );

    CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_capability_revocations_active_subject
        ON agent_capability_revocations(project_id, subject_kind, subject_id)
        WHERE status = 'active';
    CREATE INDEX IF NOT EXISTS idx_agent_capability_revocations_status
        ON agent_capability_revocations(project_id, status, created_at DESC, id DESC);
"#;

const MIGRATION_018_AGENT_SUBAGENT_TASKS_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS agent_subagent_tasks (
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        parent_run_id TEXT NOT NULL,
        subagent_id TEXT NOT NULL,
        role TEXT NOT NULL,
        role_label TEXT NOT NULL,
        prompt_hash TEXT NOT NULL,
        prompt_preview TEXT NOT NULL DEFAULT '',
        model_id TEXT,
        write_set_json TEXT NOT NULL DEFAULT '[]' CHECK (write_set_json <> '' AND json_valid(write_set_json)),
        verification_contract TEXT NOT NULL,
        depth INTEGER NOT NULL DEFAULT 0 CHECK (depth >= 0),
        max_tool_calls INTEGER NOT NULL DEFAULT 0 CHECK (max_tool_calls >= 0),
        max_tokens INTEGER NOT NULL DEFAULT 0 CHECK (max_tokens >= 0),
        max_cost_micros INTEGER NOT NULL DEFAULT 0 CHECK (max_cost_micros >= 0),
        used_tool_calls INTEGER NOT NULL DEFAULT 0 CHECK (used_tool_calls >= 0),
        used_tokens INTEGER NOT NULL DEFAULT 0 CHECK (used_tokens >= 0),
        used_cost_micros INTEGER NOT NULL DEFAULT 0 CHECK (used_cost_micros >= 0),
        budget_status TEXT NOT NULL DEFAULT 'within_budget',
        budget_diagnostic_json TEXT CHECK (budget_diagnostic_json IS NULL OR (budget_diagnostic_json <> '' AND json_valid(budget_diagnostic_json))),
        status TEXT NOT NULL,
        created_at TEXT NOT NULL,
        started_at TEXT,
        completed_at TEXT,
        cancelled_at TEXT,
        integrated_at TEXT,
        child_run_id TEXT,
        child_trace_id TEXT,
        parent_trace_id TEXT,
        input_log_json TEXT NOT NULL DEFAULT '[]' CHECK (input_log_json <> '' AND json_valid(input_log_json)),
        result_summary TEXT,
        result_artifact TEXT,
        parent_decision TEXT,
        latest_summary TEXT,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (project_id, parent_run_id, subagent_id),
        CHECK (parent_run_id <> ''),
        CHECK (subagent_id <> ''),
        CHECK (role IN ('engineer', 'debugger', 'planner', 'researcher', 'reviewer', 'agent_builder', 'browser', 'emulator', 'solana', 'database')),
        CHECK (role_label <> ''),
        CHECK (length(prompt_hash) = 64 AND prompt_hash NOT GLOB '*[^0-9a-f]*'),
        CHECK (model_id IS NULL OR model_id <> ''),
        CHECK (verification_contract <> ''),
        CHECK (budget_status IN ('within_budget', 'tool_calls_exhausted', 'tokens_exhausted', 'cost_exhausted')),
        CHECK (status IN ('none', 'registered', 'starting', 'running', 'paused', 'cancelling', 'cancelled', 'handed_off', 'completed', 'failed', 'interrupted', 'closed', 'budget_exhausted')),
        CHECK (child_run_id IS NULL OR child_run_id <> ''),
        CHECK (child_trace_id IS NULL OR (length(child_trace_id) = 32 AND child_trace_id NOT GLOB '*[^0-9a-f]*')),
        CHECK (parent_trace_id IS NULL OR (length(parent_trace_id) = 32 AND parent_trace_id NOT GLOB '*[^0-9a-f]*')),
        CHECK (result_artifact IS NULL OR result_artifact <> ''),
        CHECK (parent_decision IS NULL OR parent_decision <> ''),
        CHECK (
            (status IN ('registered', 'starting', 'running', 'paused', 'cancelling') AND completed_at IS NULL)
            OR status NOT IN ('registered', 'starting', 'running', 'paused', 'cancelling')
        ),
        FOREIGN KEY (project_id, parent_run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_agent_subagent_tasks_parent_status
        ON agent_subagent_tasks(project_id, parent_run_id, status, updated_at DESC);
    CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_subagent_tasks_child
        ON agent_subagent_tasks(project_id, child_run_id)
        WHERE child_run_id IS NOT NULL;
"#;

const MIGRATION_005_AGENT_PLAN_PACKS_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS agent_plan_sessions (
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        plan_session_id TEXT NOT NULL,
        agent_session_id TEXT NOT NULL,
        source_run_id TEXT,
        status TEXT NOT NULL,
        title TEXT NOT NULL,
        goal TEXT NOT NULL DEFAULT '',
        selected INTEGER NOT NULL DEFAULT 0 CHECK (selected IN (0, 1)),
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        accepted_at TEXT,
        PRIMARY KEY (project_id, plan_session_id),
        CHECK (plan_session_id <> ''),
        CHECK (agent_session_id <> ''),
        CHECK (source_run_id IS NULL OR source_run_id <> ''),
        CHECK (status IN ('intake', 'draft', 'accepted', 'building', 'completed', 'cancelled', 'superseded')),
        CHECK (title <> ''),
        CHECK (accepted_at IS NULL OR status IN ('accepted', 'building', 'completed', 'superseded')),
        FOREIGN KEY (project_id, agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, source_run_id)
            REFERENCES agent_runs(project_id, run_id)
    );

    CREATE INDEX IF NOT EXISTS idx_agent_plan_sessions_project_selected
        ON agent_plan_sessions(project_id, selected, updated_at DESC);
    CREATE INDEX IF NOT EXISTS idx_agent_plan_sessions_source_run
        ON agent_plan_sessions(project_id, source_run_id)
        WHERE source_run_id IS NOT NULL;

    CREATE TABLE IF NOT EXISTS agent_plan_packs (
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        plan_id TEXT NOT NULL,
        plan_session_id TEXT NOT NULL,
        schema_name TEXT NOT NULL DEFAULT 'xero.plan_pack.v1',
        status TEXT NOT NULL,
        agent_session_id TEXT NOT NULL,
        source_run_id TEXT,
        source_agent_session_id TEXT,
        title TEXT NOT NULL,
        goal TEXT NOT NULL,
        markdown TEXT NOT NULL,
        pack_json TEXT NOT NULL,
        accepted_revision INTEGER NOT NULL DEFAULT 0 CHECK (accepted_revision >= 0),
        lance_record_id TEXT,
        supersedes_plan_id TEXT,
        superseded_by_plan_id TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        accepted_at TEXT,
        PRIMARY KEY (project_id, plan_id),
        CHECK (plan_id <> ''),
        CHECK (schema_name = 'xero.plan_pack.v1'),
        CHECK (status IN ('draft', 'accepted', 'building', 'completed', 'superseded')),
        CHECK (agent_session_id <> ''),
        CHECK (source_run_id IS NULL OR source_run_id <> ''),
        CHECK (source_agent_session_id IS NULL OR source_agent_session_id <> ''),
        CHECK (title <> ''),
        CHECK (goal <> ''),
        CHECK (markdown <> ''),
        CHECK (pack_json <> '' AND json_valid(pack_json)),
        CHECK (lance_record_id IS NULL OR lance_record_id <> ''),
        CHECK (supersedes_plan_id IS NULL OR supersedes_plan_id <> ''),
        CHECK (superseded_by_plan_id IS NULL OR superseded_by_plan_id <> ''),
        CHECK (accepted_at IS NULL OR status IN ('accepted', 'building', 'completed', 'superseded')),
        FOREIGN KEY (project_id, plan_session_id)
            REFERENCES agent_plan_sessions(project_id, plan_session_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, source_run_id)
            REFERENCES agent_runs(project_id, run_id),
        FOREIGN KEY (project_id, source_agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id),
        FOREIGN KEY (project_id, supersedes_plan_id)
            REFERENCES agent_plan_packs(project_id, plan_id),
        FOREIGN KEY (project_id, superseded_by_plan_id)
            REFERENCES agent_plan_packs(project_id, plan_id)
    );

    CREATE INDEX IF NOT EXISTS idx_agent_plan_packs_session_status
        ON agent_plan_packs(project_id, plan_session_id, status, updated_at DESC);
    CREATE INDEX IF NOT EXISTS idx_agent_plan_packs_lance_record
        ON agent_plan_packs(project_id, lance_record_id)
        WHERE lance_record_id IS NOT NULL;

    CREATE TABLE IF NOT EXISTS agent_plan_slices (
        project_id TEXT NOT NULL,
        plan_id TEXT NOT NULL,
        slice_id TEXT NOT NULL,
        phase_id TEXT NOT NULL,
        phase_title TEXT NOT NULL,
        ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
        title TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'pending',
        purpose TEXT NOT NULL DEFAULT '',
        scope TEXT NOT NULL DEFAULT '',
        dependencies_json TEXT NOT NULL DEFAULT '[]',
        implementation_json TEXT NOT NULL DEFAULT '[]',
        acceptance_json TEXT NOT NULL DEFAULT '[]',
        verification_json TEXT NOT NULL DEFAULT '[]',
        handoff_notes TEXT NOT NULL DEFAULT '',
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (project_id, plan_id, slice_id),
        CHECK (slice_id <> ''),
        CHECK (phase_id <> ''),
        CHECK (phase_title <> ''),
        CHECK (title <> ''),
        CHECK (status IN ('pending', 'in_progress', 'completed', 'blocked', 'skipped')),
        CHECK (dependencies_json <> '' AND json_valid(dependencies_json)),
        CHECK (implementation_json <> '' AND json_valid(implementation_json)),
        CHECK (acceptance_json <> '' AND json_valid(acceptance_json)),
        CHECK (verification_json <> '' AND json_valid(verification_json)),
        FOREIGN KEY (project_id, plan_id)
            REFERENCES agent_plan_packs(project_id, plan_id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_agent_plan_slices_plan_order
        ON agent_plan_slices(project_id, plan_id, ordinal, slice_id);

    CREATE TABLE IF NOT EXISTS agent_plan_responses (
        project_id TEXT NOT NULL,
        plan_session_id TEXT NOT NULL,
        action_id TEXT NOT NULL,
        question_id TEXT NOT NULL,
        answer_shape TEXT NOT NULL,
        raw_answer TEXT NOT NULL DEFAULT '',
        normalized_answer_json TEXT NOT NULL DEFAULT 'null',
        created_at TEXT NOT NULL,
        resolved_at TEXT,
        PRIMARY KEY (project_id, plan_session_id, action_id),
        CHECK (action_id <> ''),
        CHECK (question_id <> ''),
        CHECK (answer_shape IN ('plain_text', 'terminal_input', 'single_choice', 'multi_choice', 'short_text', 'long_text', 'number', 'date')),
        CHECK (normalized_answer_json <> '' AND json_valid(normalized_answer_json)),
        FOREIGN KEY (project_id, plan_session_id)
            REFERENCES agent_plan_sessions(project_id, plan_session_id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_agent_plan_responses_question
        ON agent_plan_responses(project_id, plan_session_id, question_id, created_at);

    CREATE TABLE IF NOT EXISTS agent_plan_handoffs (
        project_id TEXT NOT NULL,
        plan_id TEXT NOT NULL,
        idempotency_key TEXT NOT NULL,
        target_runtime_agent_id TEXT NOT NULL,
        target_agent_session_id TEXT,
        target_run_id TEXT,
        start_slice_id TEXT,
        handoff_status TEXT NOT NULL DEFAULT 'pending',
        engineer_prompt TEXT NOT NULL,
        seed_todo_items_json TEXT NOT NULL DEFAULT '[]',
        plan_mode_satisfied INTEGER NOT NULL DEFAULT 0 CHECK (plan_mode_satisfied IN (0, 1)),
        diagnostic TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (project_id, idempotency_key),
        CHECK (idempotency_key <> ''),
        CHECK (target_runtime_agent_id <> ''),
        CHECK (target_agent_session_id IS NULL OR target_agent_session_id <> ''),
        CHECK (target_run_id IS NULL OR target_run_id <> ''),
        CHECK (start_slice_id IS NULL OR start_slice_id <> ''),
        CHECK (handoff_status IN ('pending', 'started', 'completed', 'failed', 'cancelled')),
        CHECK (engineer_prompt <> ''),
        CHECK (seed_todo_items_json <> '' AND json_valid(seed_todo_items_json)),
        FOREIGN KEY (project_id, plan_id)
            REFERENCES agent_plan_packs(project_id, plan_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, target_agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id),
        FOREIGN KEY (project_id, target_run_id)
            REFERENCES agent_runs(project_id, run_id)
    );

    CREATE INDEX IF NOT EXISTS idx_agent_plan_handoffs_plan
        ON agent_plan_handoffs(project_id, plan_id, handoff_status, updated_at DESC);
"#;

const MIGRATION_010_CODE_ROLLBACK_STORAGE_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS code_blobs (
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        blob_id TEXT NOT NULL,
        sha256 TEXT NOT NULL,
        size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
        storage_path TEXT NOT NULL,
        compression TEXT NOT NULL DEFAULT 'none',
        created_at TEXT NOT NULL,
        PRIMARY KEY (project_id, blob_id),
        CHECK (length(blob_id) = 64 AND blob_id NOT GLOB '*[^0-9a-f]*'),
        CHECK (sha256 = blob_id),
        CHECK (storage_path <> ''),
        CHECK (compression IN ('none'))
    );

    CREATE TABLE IF NOT EXISTS code_snapshots (
        project_id TEXT NOT NULL,
        agent_session_id TEXT NOT NULL,
        run_id TEXT NOT NULL,
        snapshot_id TEXT NOT NULL,
        change_group_id TEXT,
        boundary_kind TEXT NOT NULL,
        root_path TEXT NOT NULL,
        manifest_json TEXT NOT NULL DEFAULT '{}',
        write_state TEXT NOT NULL,
        entry_count INTEGER NOT NULL DEFAULT 0 CHECK (entry_count >= 0),
        total_file_bytes INTEGER NOT NULL DEFAULT 0 CHECK (total_file_bytes >= 0),
        diagnostic_json TEXT,
        created_at TEXT NOT NULL,
        completed_at TEXT,
        PRIMARY KEY (project_id, snapshot_id),
        CHECK (agent_session_id <> ''),
        CHECK (run_id <> ''),
        CHECK (snapshot_id <> ''),
        CHECK (change_group_id IS NULL OR change_group_id <> ''),
        CHECK (boundary_kind IN ('before', 'after', 'baseline', 'pre_rollback', 'post_rollback', 'manual')),
        CHECK (root_path <> ''),
        CHECK (manifest_json <> '' AND json_valid(manifest_json)),
        CHECK (write_state IN ('pending', 'completed', 'failed')),
        CHECK (diagnostic_json IS NULL OR (diagnostic_json <> '' AND json_valid(diagnostic_json))),
        CHECK (
            (write_state = 'pending' AND completed_at IS NULL)
            OR (write_state IN ('completed', 'failed') AND completed_at IS NOT NULL)
        ),
        FOREIGN KEY (project_id, agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_code_snapshots_project_run
        ON code_snapshots(project_id, run_id, created_at DESC);
    CREATE INDEX IF NOT EXISTS idx_code_snapshots_change_group
        ON code_snapshots(project_id, change_group_id, boundary_kind);

    CREATE TABLE IF NOT EXISTS code_change_groups (
        project_id TEXT NOT NULL,
        agent_session_id TEXT NOT NULL,
        run_id TEXT NOT NULL,
        change_group_id TEXT NOT NULL,
        parent_change_group_id TEXT,
        tool_call_id TEXT,
        runtime_event_id INTEGER,
        conversation_sequence INTEGER,
        change_kind TEXT NOT NULL,
        summary_label TEXT NOT NULL,
        before_snapshot_id TEXT,
        after_snapshot_id TEXT,
        restore_state TEXT NOT NULL,
        status TEXT NOT NULL,
        diagnostic_json TEXT,
        started_at TEXT NOT NULL,
        completed_at TEXT,
        PRIMARY KEY (project_id, change_group_id),
        CHECK (agent_session_id <> ''),
        CHECK (run_id <> ''),
        CHECK (change_group_id <> ''),
        CHECK (parent_change_group_id IS NULL OR parent_change_group_id <> ''),
        CHECK (tool_call_id IS NULL OR tool_call_id <> ''),
        CHECK (runtime_event_id IS NULL OR runtime_event_id > 0),
        CHECK (conversation_sequence IS NULL OR conversation_sequence >= 0),
        CHECK (change_kind IN ('file_tool', 'command', 'mcp', 'rollback', 'recovered_mutation', 'imported_baseline')),
        CHECK (summary_label <> ''),
        CHECK (before_snapshot_id IS NULL OR before_snapshot_id <> ''),
        CHECK (after_snapshot_id IS NULL OR after_snapshot_id <> ''),
        CHECK (restore_state IN ('snapshot_available', 'snapshot_missing', 'external_effects_untracked')),
        CHECK (status IN ('open', 'completed', 'superseded', 'rolled_back', 'failed')),
        CHECK (diagnostic_json IS NULL OR (diagnostic_json <> '' AND json_valid(diagnostic_json))),
        CHECK (
            (status = 'open' AND completed_at IS NULL)
            OR (status <> 'open' AND completed_at IS NOT NULL)
        ),
        FOREIGN KEY (project_id, agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, before_snapshot_id)
            REFERENCES code_snapshots(project_id, snapshot_id) ON DELETE SET NULL,
        FOREIGN KEY (project_id, after_snapshot_id)
            REFERENCES code_snapshots(project_id, snapshot_id) ON DELETE SET NULL
    );

    CREATE INDEX IF NOT EXISTS idx_code_change_groups_session
        ON code_change_groups(project_id, agent_session_id, started_at DESC);
    CREATE INDEX IF NOT EXISTS idx_code_change_groups_run
        ON code_change_groups(project_id, run_id, started_at DESC);
    CREATE INDEX IF NOT EXISTS idx_code_change_groups_tool_call
        ON code_change_groups(project_id, run_id, tool_call_id)
        WHERE tool_call_id IS NOT NULL;

    CREATE TABLE IF NOT EXISTS code_file_versions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        project_id TEXT NOT NULL,
        change_group_id TEXT NOT NULL,
        path_before TEXT,
        path_after TEXT,
        operation TEXT NOT NULL,
        before_file_kind TEXT,
        after_file_kind TEXT,
        before_hash TEXT,
        after_hash TEXT,
        before_blob_id TEXT,
        after_blob_id TEXT,
        before_size INTEGER,
        after_size INTEGER,
        before_mode INTEGER,
        after_mode INTEGER,
        before_symlink_target TEXT,
        after_symlink_target TEXT,
        explicitly_edited INTEGER NOT NULL DEFAULT 1 CHECK (explicitly_edited IN (0, 1)),
        generated INTEGER NOT NULL DEFAULT 0 CHECK (generated IN (0, 1)),
        created_at TEXT NOT NULL,
        CHECK (path_before IS NOT NULL OR path_after IS NOT NULL),
        CHECK (path_before IS NULL OR path_before <> ''),
        CHECK (path_after IS NULL OR path_after <> ''),
        CHECK (operation IN ('create', 'modify', 'delete', 'rename', 'mode_change', 'symlink_change')),
        CHECK (before_file_kind IS NULL OR before_file_kind IN ('file', 'directory', 'symlink')),
        CHECK (after_file_kind IS NULL OR after_file_kind IN ('file', 'directory', 'symlink')),
        CHECK (before_hash IS NULL OR (length(before_hash) = 64 AND before_hash NOT GLOB '*[^0-9a-f]*')),
        CHECK (after_hash IS NULL OR (length(after_hash) = 64 AND after_hash NOT GLOB '*[^0-9a-f]*')),
        CHECK (before_blob_id IS NULL OR (length(before_blob_id) = 64 AND before_blob_id NOT GLOB '*[^0-9a-f]*')),
        CHECK (after_blob_id IS NULL OR (length(after_blob_id) = 64 AND after_blob_id NOT GLOB '*[^0-9a-f]*')),
        CHECK (before_size IS NULL OR before_size >= 0),
        CHECK (after_size IS NULL OR after_size >= 0),
        FOREIGN KEY (project_id, change_group_id)
            REFERENCES code_change_groups(project_id, change_group_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, before_blob_id)
            REFERENCES code_blobs(project_id, blob_id) ON DELETE RESTRICT,
        FOREIGN KEY (project_id, after_blob_id)
            REFERENCES code_blobs(project_id, blob_id) ON DELETE RESTRICT
    );

    CREATE INDEX IF NOT EXISTS idx_code_file_versions_change_group
        ON code_file_versions(project_id, change_group_id, id ASC);
    CREATE INDEX IF NOT EXISTS idx_code_file_versions_paths
        ON code_file_versions(project_id, path_before, path_after);

    CREATE TABLE IF NOT EXISTS code_rollback_operations (
        project_id TEXT NOT NULL,
        operation_id TEXT NOT NULL,
        agent_session_id TEXT NOT NULL,
        run_id TEXT NOT NULL,
        target_change_group_id TEXT NOT NULL,
        target_snapshot_id TEXT NOT NULL,
        pre_rollback_snapshot_id TEXT,
        result_change_group_id TEXT,
        status TEXT NOT NULL,
        failure_code TEXT,
        failure_message TEXT,
        affected_files_json TEXT NOT NULL DEFAULT '[]',
        created_at TEXT NOT NULL,
        completed_at TEXT,
        PRIMARY KEY (project_id, operation_id),
        CHECK (operation_id <> ''),
        CHECK (agent_session_id <> ''),
        CHECK (run_id <> ''),
        CHECK (target_change_group_id <> ''),
        CHECK (target_snapshot_id <> ''),
        CHECK (pre_rollback_snapshot_id IS NULL OR pre_rollback_snapshot_id <> ''),
        CHECK (result_change_group_id IS NULL OR result_change_group_id <> ''),
        CHECK (status IN ('pending', 'completed', 'failed')),
        CHECK (failure_code IS NULL OR failure_code <> ''),
        CHECK (failure_message IS NULL OR failure_message <> ''),
        CHECK (affected_files_json <> '' AND json_valid(affected_files_json)),
        CHECK (
            (status = 'pending' AND completed_at IS NULL)
            OR (status IN ('completed', 'failed') AND completed_at IS NOT NULL)
        ),
        CHECK (
            (status = 'failed' AND failure_code IS NOT NULL AND failure_message IS NOT NULL)
            OR (status <> 'failed' AND failure_code IS NULL AND failure_message IS NULL)
        ),
        FOREIGN KEY (project_id, agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, target_change_group_id)
            REFERENCES code_change_groups(project_id, change_group_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, target_snapshot_id)
            REFERENCES code_snapshots(project_id, snapshot_id) ON DELETE RESTRICT,
        FOREIGN KEY (project_id, pre_rollback_snapshot_id)
            REFERENCES code_snapshots(project_id, snapshot_id) ON DELETE SET NULL,
        FOREIGN KEY (project_id, result_change_group_id)
            REFERENCES code_change_groups(project_id, change_group_id) ON DELETE SET NULL
    );

    CREATE INDEX IF NOT EXISTS idx_code_rollback_operations_session
        ON code_rollback_operations(project_id, agent_session_id, created_at DESC);
    CREATE INDEX IF NOT EXISTS idx_code_rollback_operations_target
        ON code_rollback_operations(project_id, target_change_group_id, created_at DESC);
"#;

const MIGRATION_011_CODE_HISTORY_WORKSPACE_HEAD_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS code_workspace_heads (
        project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
        head_id TEXT,
        tree_id TEXT,
        workspace_epoch INTEGER NOT NULL DEFAULT 0 CHECK (workspace_epoch >= 0),
        latest_history_operation_id TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        CHECK (head_id IS NULL OR head_id <> ''),
        CHECK (tree_id IS NULL OR tree_id <> ''),
        CHECK (latest_history_operation_id IS NULL OR latest_history_operation_id <> '')
    );

    CREATE TABLE IF NOT EXISTS code_path_epochs (
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        path TEXT NOT NULL,
        workspace_epoch INTEGER NOT NULL CHECK (workspace_epoch >= 0),
        commit_id TEXT,
        history_operation_id TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (project_id, path),
        CHECK (path <> ''),
        CHECK (commit_id IS NULL OR commit_id <> ''),
        CHECK (history_operation_id IS NULL OR history_operation_id <> '')
    );

    CREATE INDEX IF NOT EXISTS idx_code_path_epochs_project_epoch
        ON code_path_epochs(project_id, workspace_epoch DESC, path);
    CREATE INDEX IF NOT EXISTS idx_code_path_epochs_commit
        ON code_path_epochs(project_id, commit_id)
        WHERE commit_id IS NOT NULL;
"#;

const MIGRATION_012_CODE_HISTORY_COMMIT_PATCHSET_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS code_patchsets (
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        patchset_id TEXT NOT NULL,
        change_group_id TEXT NOT NULL,
        base_commit_id TEXT,
        base_tree_id TEXT,
        result_tree_id TEXT NOT NULL,
        patch_kind TEXT NOT NULL,
        file_count INTEGER NOT NULL CHECK (file_count >= 0),
        text_hunk_count INTEGER NOT NULL CHECK (text_hunk_count >= 0),
        created_at TEXT NOT NULL,
        PRIMARY KEY (project_id, patchset_id),
        CHECK (patchset_id <> ''),
        CHECK (change_group_id <> ''),
        CHECK (base_commit_id IS NULL OR base_commit_id <> ''),
        CHECK (base_tree_id IS NULL OR base_tree_id <> ''),
        CHECK (result_tree_id <> ''),
        CHECK (patch_kind IN ('change_group', 'undo', 'session_rollback', 'recovered_mutation', 'imported_baseline')),
        FOREIGN KEY (project_id, change_group_id)
            REFERENCES code_change_groups(project_id, change_group_id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_code_patchsets_change_group
        ON code_patchsets(project_id, change_group_id, created_at DESC);
    CREATE INDEX IF NOT EXISTS idx_code_patchsets_base_commit
        ON code_patchsets(project_id, base_commit_id)
        WHERE base_commit_id IS NOT NULL;

    CREATE TABLE IF NOT EXISTS code_commits (
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        commit_id TEXT NOT NULL,
        parent_commit_id TEXT,
        tree_id TEXT NOT NULL,
        parent_tree_id TEXT,
        patchset_id TEXT NOT NULL,
        change_group_id TEXT NOT NULL,
        history_operation_id TEXT,
        agent_session_id TEXT NOT NULL,
        run_id TEXT NOT NULL,
        tool_call_id TEXT,
        runtime_event_id INTEGER,
        conversation_sequence INTEGER,
        commit_kind TEXT NOT NULL,
        summary_label TEXT NOT NULL,
        workspace_epoch INTEGER NOT NULL CHECK (workspace_epoch >= 0),
        created_at TEXT NOT NULL,
        completed_at TEXT NOT NULL,
        PRIMARY KEY (project_id, commit_id),
        CHECK (commit_id <> ''),
        CHECK (parent_commit_id IS NULL OR parent_commit_id <> ''),
        CHECK (tree_id <> ''),
        CHECK (parent_tree_id IS NULL OR parent_tree_id <> ''),
        CHECK (patchset_id <> ''),
        CHECK (change_group_id <> ''),
        CHECK (history_operation_id IS NULL OR history_operation_id <> ''),
        CHECK (agent_session_id <> ''),
        CHECK (run_id <> ''),
        CHECK (tool_call_id IS NULL OR tool_call_id <> ''),
        CHECK (runtime_event_id IS NULL OR runtime_event_id > 0),
        CHECK (conversation_sequence IS NULL OR conversation_sequence >= 0),
        CHECK (commit_kind IN ('change_group', 'undo', 'session_rollback', 'recovered_mutation', 'imported_baseline')),
        CHECK (summary_label <> ''),
        FOREIGN KEY (project_id, parent_commit_id)
            REFERENCES code_commits(project_id, commit_id) ON DELETE SET NULL,
        FOREIGN KEY (project_id, patchset_id)
            REFERENCES code_patchsets(project_id, patchset_id) ON DELETE RESTRICT,
        FOREIGN KEY (project_id, change_group_id)
            REFERENCES code_change_groups(project_id, change_group_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_code_commits_change_group
        ON code_commits(project_id, change_group_id, completed_at DESC);
    CREATE INDEX IF NOT EXISTS idx_code_commits_session
        ON code_commits(project_id, agent_session_id, completed_at DESC);
    CREATE INDEX IF NOT EXISTS idx_code_commits_run
        ON code_commits(project_id, run_id, completed_at DESC);
    CREATE INDEX IF NOT EXISTS idx_code_commits_patchset
        ON code_commits(project_id, patchset_id);

    CREATE TABLE IF NOT EXISTS code_patch_files (
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        patchset_id TEXT NOT NULL,
        patch_file_id TEXT NOT NULL,
        file_index INTEGER NOT NULL CHECK (file_index >= 0),
        path_before TEXT,
        path_after TEXT,
        operation TEXT NOT NULL,
        merge_policy TEXT NOT NULL,
        before_file_kind TEXT,
        after_file_kind TEXT,
        base_hash TEXT,
        result_hash TEXT,
        base_blob_id TEXT,
        result_blob_id TEXT,
        base_size INTEGER,
        result_size INTEGER,
        base_mode INTEGER,
        result_mode INTEGER,
        base_symlink_target TEXT,
        result_symlink_target TEXT,
        text_hunk_count INTEGER NOT NULL CHECK (text_hunk_count >= 0),
        created_at TEXT NOT NULL,
        PRIMARY KEY (project_id, patch_file_id),
        UNIQUE (project_id, patchset_id, file_index),
        CHECK (patch_file_id <> ''),
        CHECK (path_before IS NOT NULL OR path_after IS NOT NULL),
        CHECK (path_before IS NULL OR path_before <> ''),
        CHECK (path_after IS NULL OR path_after <> ''),
        CHECK (operation IN ('create', 'modify', 'delete', 'rename', 'mode_change', 'symlink_change')),
        CHECK (merge_policy IN ('text', 'exact')),
        CHECK (before_file_kind IS NULL OR before_file_kind IN ('file', 'directory', 'symlink')),
        CHECK (after_file_kind IS NULL OR after_file_kind IN ('file', 'directory', 'symlink')),
        CHECK (base_hash IS NULL OR (length(base_hash) = 64 AND base_hash NOT GLOB '*[^0-9a-f]*')),
        CHECK (result_hash IS NULL OR (length(result_hash) = 64 AND result_hash NOT GLOB '*[^0-9a-f]*')),
        CHECK (base_blob_id IS NULL OR (length(base_blob_id) = 64 AND base_blob_id NOT GLOB '*[^0-9a-f]*')),
        CHECK (result_blob_id IS NULL OR (length(result_blob_id) = 64 AND result_blob_id NOT GLOB '*[^0-9a-f]*')),
        CHECK (base_size IS NULL OR base_size >= 0),
        CHECK (result_size IS NULL OR result_size >= 0),
        FOREIGN KEY (project_id, patchset_id)
            REFERENCES code_patchsets(project_id, patchset_id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, base_blob_id)
            REFERENCES code_blobs(project_id, blob_id) ON DELETE RESTRICT,
        FOREIGN KEY (project_id, result_blob_id)
            REFERENCES code_blobs(project_id, blob_id) ON DELETE RESTRICT
    );

    CREATE INDEX IF NOT EXISTS idx_code_patch_files_patchset
        ON code_patch_files(project_id, patchset_id, file_index ASC);
    CREATE INDEX IF NOT EXISTS idx_code_patch_files_paths
        ON code_patch_files(project_id, path_before, path_after);

    CREATE TABLE IF NOT EXISTS code_patch_hunks (
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        patch_file_id TEXT NOT NULL,
        hunk_id TEXT NOT NULL,
        hunk_index INTEGER NOT NULL CHECK (hunk_index >= 0),
        base_start_line INTEGER NOT NULL CHECK (base_start_line >= 0),
        base_line_count INTEGER NOT NULL CHECK (base_line_count >= 0),
        result_start_line INTEGER NOT NULL CHECK (result_start_line >= 0),
        result_line_count INTEGER NOT NULL CHECK (result_line_count >= 0),
        removed_lines_json TEXT NOT NULL DEFAULT '[]' CHECK (removed_lines_json <> '' AND json_valid(removed_lines_json)),
        added_lines_json TEXT NOT NULL DEFAULT '[]' CHECK (added_lines_json <> '' AND json_valid(added_lines_json)),
        context_before_json TEXT NOT NULL DEFAULT '[]' CHECK (context_before_json <> '' AND json_valid(context_before_json)),
        context_after_json TEXT NOT NULL DEFAULT '[]' CHECK (context_after_json <> '' AND json_valid(context_after_json)),
        created_at TEXT NOT NULL,
        PRIMARY KEY (project_id, patch_file_id, hunk_id),
        CHECK (patch_file_id <> ''),
        CHECK (hunk_id <> ''),
        FOREIGN KEY (project_id, patch_file_id)
            REFERENCES code_patch_files(project_id, patch_file_id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_code_patch_hunks_file
        ON code_patch_hunks(project_id, patch_file_id, hunk_index ASC);
"#;

const MIGRATION_013_CODE_HISTORY_OPERATIONS_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS code_history_operations (
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        operation_id TEXT NOT NULL,
        mode TEXT NOT NULL,
        status TEXT NOT NULL,
        target_kind TEXT NOT NULL,
        target_id TEXT NOT NULL,
        target_change_group_id TEXT,
        target_file_path TEXT,
        target_hunk_ids_json TEXT NOT NULL DEFAULT '[]',
        agent_session_id TEXT,
        run_id TEXT,
        expected_workspace_epoch INTEGER,
        affected_paths_json TEXT NOT NULL DEFAULT '[]',
        conflicts_json TEXT NOT NULL DEFAULT '[]',
        result_change_group_id TEXT,
        result_commit_id TEXT,
        failure_code TEXT,
        failure_message TEXT,
        repair_code TEXT,
        repair_message TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        completed_at TEXT,
        PRIMARY KEY (project_id, operation_id),
        CHECK (operation_id <> ''),
        CHECK (mode IN ('selective_undo', 'session_rollback')),
        CHECK (status IN ('pending', 'planning', 'conflicted', 'applying', 'completed', 'failed', 'repair_needed')),
        CHECK (target_kind IN ('change_group', 'file_change', 'hunks', 'session_boundary', 'run_boundary')),
        CHECK (target_id <> ''),
        CHECK (target_change_group_id IS NULL OR target_change_group_id <> ''),
        CHECK (target_file_path IS NULL OR target_file_path <> ''),
        CHECK (target_hunk_ids_json <> '' AND json_valid(target_hunk_ids_json)),
        CHECK (agent_session_id IS NULL OR agent_session_id <> ''),
        CHECK (run_id IS NULL OR run_id <> ''),
        CHECK (expected_workspace_epoch IS NULL OR expected_workspace_epoch >= 0),
        CHECK (affected_paths_json <> '' AND json_valid(affected_paths_json)),
        CHECK (conflicts_json <> '' AND json_valid(conflicts_json)),
        CHECK (result_change_group_id IS NULL OR result_change_group_id <> ''),
        CHECK (result_commit_id IS NULL OR result_commit_id <> ''),
        CHECK (failure_code IS NULL OR failure_code <> ''),
        CHECK (failure_message IS NULL OR failure_message <> ''),
        CHECK (repair_code IS NULL OR repair_code <> ''),
        CHECK (repair_message IS NULL OR repair_message <> ''),
        CHECK (
            (status IN ('pending', 'planning', 'applying') AND completed_at IS NULL)
            OR (status IN ('conflicted', 'completed', 'failed', 'repair_needed') AND completed_at IS NOT NULL)
        ),
        CHECK (
            (status = 'failed' AND failure_code IS NOT NULL AND failure_message IS NOT NULL)
            OR (status <> 'failed')
        ),
        CHECK (
            (status = 'repair_needed' AND repair_code IS NOT NULL AND repair_message IS NOT NULL)
            OR (status <> 'repair_needed')
        ),
        FOREIGN KEY (project_id, target_change_group_id)
            REFERENCES code_change_groups(project_id, change_group_id) ON DELETE SET NULL,
        FOREIGN KEY (project_id, result_change_group_id)
            REFERENCES code_change_groups(project_id, change_group_id) ON DELETE SET NULL,
        FOREIGN KEY (project_id, result_commit_id)
            REFERENCES code_commits(project_id, commit_id) ON DELETE SET NULL
    );

    CREATE INDEX IF NOT EXISTS idx_code_history_operations_project_status
        ON code_history_operations(project_id, status, updated_at DESC);
    CREATE INDEX IF NOT EXISTS idx_code_history_operations_target
        ON code_history_operations(project_id, target_change_group_id, created_at DESC)
        WHERE target_change_group_id IS NOT NULL;
    CREATE INDEX IF NOT EXISTS idx_code_history_operations_result_commit
        ON code_history_operations(project_id, result_commit_id)
        WHERE result_commit_id IS NOT NULL;
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
        CHECK (lifecycle_state IN ('draft', 'valid', 'active', 'archived', 'blocked')),
        CHECK (base_capability_profile IN ('observe_only', 'planning', 'repository_recon', 'engineering', 'debugging', 'agent_builder', 'harness_test'))
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

    CREATE TABLE IF NOT EXISTS agent_runtime_audit_events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        audit_id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        actor_kind TEXT NOT NULL,
        actor_id TEXT,
        action_kind TEXT NOT NULL,
        subject_kind TEXT NOT NULL,
        subject_id TEXT NOT NULL,
        run_id TEXT,
        agent_definition_id TEXT,
        agent_definition_version INTEGER,
        risk_class TEXT,
        approval_action_id TEXT,
        payload_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload_json)),
        created_at TEXT NOT NULL,
        CHECK (audit_id <> ''),
        CHECK (actor_kind IN ('system', 'user', 'agent', 'runtime')),
        CHECK (action_kind <> ''),
        CHECK (subject_kind <> ''),
        CHECK (subject_id <> ''),
        CHECK (run_id IS NULL OR run_id <> ''),
        CHECK (agent_definition_id IS NULL OR agent_definition_id <> ''),
        CHECK (agent_definition_version IS NULL OR agent_definition_version > 0),
        CHECK (risk_class IS NULL OR risk_class <> ''),
        CHECK (approval_action_id IS NULL OR approval_action_id <> ''),
        UNIQUE (project_id, audit_id)
    );

    CREATE INDEX IF NOT EXISTS idx_agent_runtime_audit_events_subject
        ON agent_runtime_audit_events(project_id, subject_kind, subject_id, created_at DESC, id DESC);
    CREATE INDEX IF NOT EXISTS idx_agent_runtime_audit_events_run
        ON agent_runtime_audit_events(project_id, run_id, created_at DESC, id DESC)
        WHERE run_id IS NOT NULL;

    CREATE TABLE IF NOT EXISTS agent_capability_revocations (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        revocation_id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        subject_kind TEXT NOT NULL,
        subject_id TEXT NOT NULL,
        scope_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(scope_json)),
        reason TEXT NOT NULL,
        created_by TEXT NOT NULL,
        status TEXT NOT NULL,
        created_at TEXT NOT NULL,
        cleared_at TEXT,
        CHECK (revocation_id <> ''),
        CHECK (subject_kind IN ('custom_agent', 'tool_pack', 'external_integration', 'browser_control', 'destructive_write')),
        CHECK (subject_id <> ''),
        CHECK (reason <> ''),
        CHECK (created_by <> ''),
        CHECK (status IN ('active', 'cleared')),
        CHECK ((status = 'active' AND cleared_at IS NULL) OR (status = 'cleared' AND cleared_at IS NOT NULL)),
        UNIQUE (project_id, revocation_id)
    );

    CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_capability_revocations_active_subject
        ON agent_capability_revocations(project_id, subject_kind, subject_id)
        WHERE status = 'active';
    CREATE INDEX IF NOT EXISTS idx_agent_capability_revocations_status
        ON agent_capability_revocations(project_id, status, created_at DESC, id DESC);

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
        ('plan', 1, 'Plan', 'Plan', 'Turn ambiguous work into an accepted, durable implementation plan without mutating repository files.', 'built_in', 'active', 'planning', '2026-05-06T00:00:00Z'),
        ('engineer', 1, 'Engineer', 'Build', 'Implement repository changes with the existing software-building toolset and safety gates.', 'built_in', 'active', 'engineering', '2026-05-01T00:00:00Z'),
        ('debug', 1, 'Debug', 'Debug', 'Investigate failures with structured evidence, hypotheses, fixes, verification, and durable debugging memory.', 'built_in', 'active', 'debugging', '2026-05-01T00:00:00Z'),
        ('crawl', 1, 'Crawl', 'Crawl', 'Map an existing repository, identify stack, tests, commands, architecture, hot spots, and durable project facts without editing files.', 'built_in', 'active', 'repository_recon', '2026-05-06T00:00:00Z'),
        ('agent_create', 1, 'Agent Create', 'Create', 'Interview the user and draft high-quality custom agent definitions.', 'built_in', 'active', 'agent_builder', '2026-05-01T00:00:00Z'),
        ('test', 1, 'Test', 'Test', 'Run the dev harness through the normal owned-agent conversation, provider, tool, stream, and persistence path.', 'built_in', 'active', 'harness_test', '2026-05-01T00:00:00Z');

    INSERT OR IGNORE INTO agent_definition_versions (
        definition_id,
        version,
        snapshot_json,
        validation_report_json,
        created_at
    )
    VALUES
        ('ask', 1, '{"id":"ask","version":1,"scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"observe_only","label":"Ask","shortLabel":"Ask"}', '{"status":"valid","source":"seed"}', '2026-05-01T00:00:00Z'),
        ('plan', 1, '{"schema":"xero.agent_definition.v1","id":"plan","version":1,"displayName":"Plan","shortLabel":"Plan","description":"Turn ambiguous work into an accepted, durable implementation plan without mutating repository files.","taskPurpose":"Interview the user, inspect project context when useful, draft a reproducible Plan Pack, and prepare Engineer handoff.","scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"planning","defaultApprovalMode":"suggest","allowedApprovalModes":["suggest"],"promptPolicy":"plan","toolPolicy":"planning","outputContract":"plan_pack","workflowContract":"Guide a user from vague task intent to an accepted xero.plan_pack.v1 without repository mutation.","finalResponseContract":"Produce the canonical Plan Pack summary and deterministic Engineer handoff prompt after acceptance.","projectDataPolicy":{"required":true,"recordKinds":["agent_handoff","project_fact","decision","constraint","plan","question","context_note","diagnostic"],"structuredSchemas":["xero.project_record.v1","xero.plan_pack.v1"],"unstructuredScopes":["answer_note","session_summary","troubleshooting_note"],"memoryCandidateKinds":["project_fact","user_preference","decision","session_summary","troubleshooting"]}}', '{"status":"valid","source":"seed"}', '2026-05-06T00:00:00Z'),
        ('engineer', 1, '{"id":"engineer","version":1,"scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"engineering","label":"Engineer","shortLabel":"Build"}', '{"status":"valid","source":"seed"}', '2026-05-01T00:00:00Z'),
        ('debug', 1, '{"id":"debug","version":1,"scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"debugging","label":"Debug","shortLabel":"Debug"}', '{"status":"valid","source":"seed"}', '2026-05-01T00:00:00Z'),
        ('crawl', 1, '{"schema":"xero.agent_definition.v1","id":"crawl","version":1,"displayName":"Crawl","shortLabel":"Crawl","description":"Map an existing repository, identify stack, tests, commands, architecture, hot spots, and durable project facts without editing files.","taskPurpose":"Read brownfield repository context and produce a structured crawl report for durable project memory.","scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"repository_recon","defaultApprovalMode":"suggest","allowedApprovalModes":["suggest"],"promptPolicy":"crawl","toolPolicy":"repository_recon","outputContract":"crawl_report","workflowContract":"Map the brownfield repository without mutating files or app state; use manifests, instructions, workspace index, safe git metadata, and read-only discovery.","finalResponseContract":"Produce a short summary plus a valid JSON crawl report payload using schema xero.project_crawl.report.v1.","projectDataPolicy":{"required":true,"recordKinds":["project_fact","constraint","finding","verification","artifact","context_note","diagnostic","question"],"structuredSchemas":["xero.project_record.v1","xero.project_crawl.report.v1","xero.project_crawl.project_overview.v1","xero.project_crawl.tech_stack.v1","xero.project_crawl.command_map.v1","xero.project_crawl.test_map.v1","xero.project_crawl.architecture_map.v1","xero.project_crawl.hotspots.v1","xero.project_crawl.constraints.v1","xero.project_crawl.unknowns.v1","xero.project_crawl.freshness.v1"],"unstructuredScopes":["answer_note","session_summary","artifact_excerpt","troubleshooting_note"],"memoryCandidateKinds":["project_fact","decision","session_summary","troubleshooting"]}}', '{"status":"valid","source":"seed"}', '2026-05-06T00:00:00Z'),
        ('agent_create', 1, '{"id":"agent_create","version":1,"scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"agent_builder","label":"Agent Create","shortLabel":"Create"}', '{"status":"valid","source":"seed"}', '2026-05-01T00:00:00Z'),
        ('test', 1, '{"schema":"xero.agent_definition.v1","id":"test","version":1,"displayName":"Test","shortLabel":"Test","description":"Run the dev harness through the normal owned-agent conversation, provider, tool, stream, and persistence path.","taskPurpose":"Trigger and report a deterministic internal harness validation run instead of fulfilling the user prompt as a normal task.","scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"harness_test","defaultApprovalMode":"suggest","allowedApprovalModes":["suggest"],"promptPolicy":"harness_test","toolPolicy":"harness_test","outputContract":"harness_test_report","workflowContract":"Use the built-in Xero harness runtime contract for this agent.","finalResponseContract":"Produce the built-in harness test report.","projectDataPolicy":{"required":true,"recordKinds":["agent_handoff","project_fact","decision","constraint","plan","finding","verification","question","artifact","context_note","diagnostic"],"structuredSchemas":["xero.project_record.v1","xero.harness_test_report.v1"],"unstructuredScopes":["answer_note","session_summary","artifact_excerpt","troubleshooting_note"],"memoryCandidateKinds":["project_fact","decision","session_summary","troubleshooting"]}}', '{"status":"valid","source":"seed"}', '2026-05-01T00:00:00Z');

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
        trace_id TEXT NOT NULL,
        lineage_kind TEXT NOT NULL DEFAULT 'top_level',
        parent_run_id TEXT,
        parent_trace_id TEXT,
        parent_subagent_id TEXT,
        subagent_role TEXT,
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
        CHECK (length(trace_id) = 32 AND trace_id NOT GLOB '*[^0-9a-f]*'),
        CHECK (lineage_kind IN ('top_level', 'subagent_child')),
        CHECK (parent_run_id IS NULL OR parent_run_id <> ''),
        CHECK (parent_trace_id IS NULL OR (length(parent_trace_id) = 32 AND parent_trace_id NOT GLOB '*[^0-9a-f]*')),
        CHECK (parent_subagent_id IS NULL OR parent_subagent_id <> ''),
        CHECK (subagent_role IS NULL OR subagent_role IN ('engineer', 'debugger', 'planner', 'researcher', 'reviewer', 'agent_builder', 'browser', 'emulator', 'solana', 'database')),
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
    CREATE INDEX IF NOT EXISTS idx_agent_runs_parent_lineage
        ON agent_runs(project_id, parent_run_id, parent_subagent_id);

    CREATE TABLE IF NOT EXISTS agent_messages (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        project_id TEXT NOT NULL,
        run_id TEXT NOT NULL,
        role TEXT NOT NULL,
        content TEXT NOT NULL,
        provider_metadata_json TEXT,
        created_at TEXT NOT NULL,
        CHECK (role IN ('system', 'developer', 'user', 'assistant', 'tool')),
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
            'run_started',
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
            'context_manifest_recorded',
            'retrieval_performed',
            'memory_candidate_captured',
            'environment_lifecycle_update',
            'sandbox_lifecycle_update',
            'action_required',
            'approval_required',
            'tool_permission_grant',
            'provider_model_changed',
            'runtime_settings_changed',
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

    CREATE TABLE IF NOT EXISTS agent_environment_lifecycle_snapshots (
        project_id TEXT NOT NULL,
        run_id TEXT NOT NULL,
        environment_id TEXT NOT NULL,
        state TEXT NOT NULL,
        previous_state TEXT,
        pending_message_count INTEGER NOT NULL DEFAULT 0 CHECK (pending_message_count >= 0),
        health_checks_json TEXT NOT NULL DEFAULT '[]' CHECK (health_checks_json <> '' AND json_valid(health_checks_json)),
        setup_steps_json TEXT NOT NULL DEFAULT '[]' CHECK (setup_steps_json <> '' AND json_valid(setup_steps_json)),
        diagnostic_json TEXT CHECK (diagnostic_json IS NULL OR (diagnostic_json <> '' AND json_valid(diagnostic_json))),
        snapshot_json TEXT NOT NULL CHECK (snapshot_json <> '' AND json_valid(snapshot_json)),
        updated_at TEXT NOT NULL,
        PRIMARY KEY (project_id, run_id),
        CHECK (environment_id <> ''),
        CHECK (state IN (
            'created',
            'waiting_for_sandbox',
            'preparing_repository',
            'loading_project_instructions',
            'running_setup_scripts',
            'setting_up_hooks',
            'setting_up_skills_plugins',
            'indexing_workspace',
            'starting_conversation',
            'ready',
            'failed',
            'paused',
            'archived'
        )),
        CHECK (
            previous_state IS NULL OR previous_state IN (
                'created',
                'waiting_for_sandbox',
                'preparing_repository',
                'loading_project_instructions',
                'running_setup_scripts',
                'setting_up_hooks',
                'setting_up_skills_plugins',
                'indexing_workspace',
                'starting_conversation',
                'ready',
                'failed',
                'paused',
                'archived'
            )
        ),
        FOREIGN KEY (project_id, run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS agent_environment_pending_messages (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        project_id TEXT NOT NULL,
        run_id TEXT NOT NULL,
        role TEXT NOT NULL,
        content TEXT NOT NULL,
        submitted_at TEXT NOT NULL,
        delivered_at TEXT,
        CHECK (role IN ('user')),
        CHECK (content <> ''),
        CHECK (delivered_at IS NULL OR delivered_at <> ''),
        FOREIGN KEY (project_id, run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_agent_environment_pending_messages_run
        ON agent_environment_pending_messages(project_id, run_id, delivered_at, id ASC);

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
        trace_id TEXT NOT NULL,
        top_level_run_id TEXT NOT NULL,
        subagent_id TEXT,
        subagent_role TEXT,
        change_group_id TEXT,
        path TEXT NOT NULL,
        operation TEXT NOT NULL,
        old_hash TEXT,
        new_hash TEXT,
        created_at TEXT NOT NULL,
        CHECK (length(trace_id) = 32 AND trace_id NOT GLOB '*[^0-9a-f]*'),
        CHECK (top_level_run_id <> ''),
        CHECK (subagent_id IS NULL OR subagent_id <> ''),
        CHECK (subagent_role IS NULL OR subagent_role IN ('engineer', 'debugger', 'planner', 'researcher', 'reviewer', 'agent_builder', 'browser', 'emulator', 'solana', 'database')),
        CHECK (change_group_id IS NULL OR change_group_id <> ''),
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

    CREATE TABLE IF NOT EXISTS cross_store_outbox (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        operation_id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        store_kind TEXT NOT NULL,
        entity_kind TEXT NOT NULL,
        entity_id TEXT NOT NULL,
        operation TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'pending',
        payload_json TEXT NOT NULL,
        diagnostic_json TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        completed_at TEXT,
        CHECK (operation_id <> ''),
        CHECK (store_kind <> ''),
        CHECK (entity_kind <> ''),
        CHECK (entity_id <> ''),
        CHECK (operation <> ''),
        CHECK (status IN ('pending', 'applied', 'failed', 'reconciled')),
        CHECK (payload_json <> '' AND json_valid(payload_json)),
        CHECK (diagnostic_json IS NULL OR (diagnostic_json <> '' AND json_valid(diagnostic_json))),
        CHECK (created_at <> ''),
        CHECK (updated_at <> ''),
        CHECK (completed_at IS NULL OR completed_at <> ''),
        UNIQUE (project_id, operation_id)
    );

    CREATE INDEX IF NOT EXISTS idx_cross_store_outbox_status
        ON cross_store_outbox(project_id, status, created_at ASC, id ASC);
    CREATE INDEX IF NOT EXISTS idx_cross_store_outbox_entity
        ON cross_store_outbox(project_id, entity_kind, entity_id, status);

    CREATE TABLE IF NOT EXISTS project_storage_maintenance_runs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        run_id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        maintenance_kind TEXT NOT NULL,
        status TEXT NOT NULL,
        diagnostic_json TEXT,
        started_at TEXT NOT NULL,
        completed_at TEXT,
        CHECK (run_id <> ''),
        CHECK (maintenance_kind <> ''),
        CHECK (status IN ('running', 'succeeded', 'failed')),
        CHECK (diagnostic_json IS NULL OR (diagnostic_json <> '' AND json_valid(diagnostic_json))),
        CHECK (started_at <> ''),
        CHECK (completed_at IS NULL OR completed_at <> ''),
        UNIQUE (project_id, run_id)
    );

    CREATE INDEX IF NOT EXISTS idx_project_storage_maintenance_runs_latest
        ON project_storage_maintenance_runs(project_id, status, completed_at DESC, id DESC);

    CREATE TABLE IF NOT EXISTS meta (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        project_id TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        CHECK (project_id <> '')
    );
"#;

fn migrate_project_origin_column(transaction: &Transaction<'_>) -> rusqlite_migration::HookResult {
    add_column_if_missing(
        transaction,
        "projects",
        "project_origin",
        "TEXT NOT NULL DEFAULT 'unknown' CHECK (project_origin IN ('brownfield', 'greenfield', 'unknown'))",
    )?;
    Ok(())
}

fn migrate_agent_trace_columns(transaction: &Transaction<'_>) -> rusqlite_migration::HookResult {
    if table_exists(transaction, "agent_runs")? {
        add_column_if_missing(
            transaction,
            "agent_runs",
            "trace_id",
            "TEXT NOT NULL DEFAULT '00000000000000000000000000000000'",
        )?;
        add_column_if_missing(
            transaction,
            "agent_runs",
            "lineage_kind",
            "TEXT NOT NULL DEFAULT 'top_level'",
        )?;
        add_column_if_missing(transaction, "agent_runs", "parent_run_id", "TEXT")?;
        add_column_if_missing(transaction, "agent_runs", "parent_trace_id", "TEXT")?;
        add_column_if_missing(transaction, "agent_runs", "parent_subagent_id", "TEXT")?;
        add_column_if_missing(transaction, "agent_runs", "subagent_role", "TEXT")?;

        if table_exists(transaction, "agent_coordination_presence")? {
            transaction.execute_batch(
                r#"
                UPDATE agent_runs
                SET trace_id = (
                    SELECT agent_coordination_presence.trace_id
                    FROM agent_coordination_presence
                    WHERE agent_coordination_presence.project_id = agent_runs.project_id
                      AND agent_coordination_presence.run_id = agent_runs.run_id
                    LIMIT 1
                )
                WHERE (
                    trace_id IS NULL
                    OR trim(trace_id) = ''
                    OR trace_id = '00000000000000000000000000000000'
                )
                  AND EXISTS (
                    SELECT 1
                    FROM agent_coordination_presence
                    WHERE agent_coordination_presence.project_id = agent_runs.project_id
                      AND agent_coordination_presence.run_id = agent_runs.run_id
                );
                "#,
            )?;
        }

        transaction.execute_batch(
            r#"
            UPDATE agent_runs
            SET trace_id = lower(hex(randomblob(16)))
            WHERE trace_id IS NULL
               OR trim(trace_id) = ''
               OR trace_id = '00000000000000000000000000000000';

            UPDATE agent_runs
            SET lineage_kind = 'top_level'
            WHERE lineage_kind IS NULL
               OR trim(lineage_kind) = '';

            CREATE INDEX IF NOT EXISTS idx_agent_runs_parent_lineage
                ON agent_runs(project_id, parent_run_id, parent_subagent_id);
            "#,
        )?;
    }

    if table_exists(transaction, "agent_file_changes")? {
        add_column_if_missing(
            transaction,
            "agent_file_changes",
            "trace_id",
            "TEXT NOT NULL DEFAULT '00000000000000000000000000000000'",
        )?;
        add_column_if_missing(
            transaction,
            "agent_file_changes",
            "top_level_run_id",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        add_column_if_missing(transaction, "agent_file_changes", "subagent_id", "TEXT")?;
        add_column_if_missing(transaction, "agent_file_changes", "subagent_role", "TEXT")?;
        add_column_if_missing(transaction, "agent_file_changes", "change_group_id", "TEXT")?;

        if table_exists(transaction, "agent_runs")? {
            transaction.execute_batch(
                r#"
                UPDATE agent_file_changes
                SET trace_id = COALESCE(
                        (
                            SELECT agent_runs.trace_id
                            FROM agent_runs
                            WHERE agent_runs.project_id = agent_file_changes.project_id
                              AND agent_runs.run_id = agent_file_changes.run_id
                        ),
                        trace_id
                    ),
                    top_level_run_id = COALESCE(
                        (
                            SELECT COALESCE(agent_runs.parent_run_id, agent_runs.run_id)
                            FROM agent_runs
                            WHERE agent_runs.project_id = agent_file_changes.project_id
                              AND agent_runs.run_id = agent_file_changes.run_id
                        ),
                        run_id
                    ),
                    subagent_id = (
                        SELECT agent_runs.parent_subagent_id
                        FROM agent_runs
                        WHERE agent_runs.project_id = agent_file_changes.project_id
                          AND agent_runs.run_id = agent_file_changes.run_id
                    ),
                    subagent_role = (
                        SELECT agent_runs.subagent_role
                        FROM agent_runs
                        WHERE agent_runs.project_id = agent_file_changes.project_id
                          AND agent_runs.run_id = agent_file_changes.run_id
                    );
                "#,
            )?;
        }

        transaction.execute_batch(
            r#"
            UPDATE agent_file_changes
            SET trace_id = lower(hex(randomblob(16)))
            WHERE trace_id IS NULL
               OR trim(trace_id) = ''
               OR trace_id = '00000000000000000000000000000000';

            UPDATE agent_file_changes
            SET top_level_run_id = run_id
            WHERE top_level_run_id IS NULL
               OR trim(top_level_run_id) = '';
            "#,
        )?;
    }

    Ok(())
}

fn migrate_agent_trace_columns_repair(
    transaction: &Transaction<'_>,
) -> rusqlite_migration::HookResult {
    migrate_agent_trace_columns(transaction)
}

fn migrate_agent_messages_provider_metadata_json(
    transaction: &Transaction<'_>,
) -> rusqlite_migration::HookResult {
    if table_exists(transaction, "agent_messages")? {
        add_column_if_missing(
            transaction,
            "agent_messages",
            "provider_metadata_json",
            "TEXT",
        )?;
    }

    Ok(())
}

fn migrate_environment_lifecycle_schema(
    transaction: &Transaction<'_>,
) -> rusqlite_migration::HookResult {
    transaction.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS agent_environment_lifecycle_snapshots (
            project_id TEXT NOT NULL,
            run_id TEXT NOT NULL,
            environment_id TEXT NOT NULL,
            state TEXT NOT NULL,
            previous_state TEXT,
            pending_message_count INTEGER NOT NULL DEFAULT 0 CHECK (pending_message_count >= 0),
            health_checks_json TEXT NOT NULL DEFAULT '[]' CHECK (health_checks_json <> '' AND json_valid(health_checks_json)),
            setup_steps_json TEXT NOT NULL DEFAULT '[]' CHECK (setup_steps_json <> '' AND json_valid(setup_steps_json)),
            diagnostic_json TEXT CHECK (diagnostic_json IS NULL OR (diagnostic_json <> '' AND json_valid(diagnostic_json))),
            snapshot_json TEXT NOT NULL CHECK (snapshot_json <> '' AND json_valid(snapshot_json)),
            updated_at TEXT NOT NULL,
            PRIMARY KEY (project_id, run_id),
            CHECK (environment_id <> ''),
            CHECK (state IN (
                'created',
                'waiting_for_sandbox',
                'preparing_repository',
                'loading_project_instructions',
                'running_setup_scripts',
                'setting_up_hooks',
                'setting_up_skills_plugins',
                'indexing_workspace',
                'starting_conversation',
                'ready',
                'failed',
                'paused',
                'archived'
            )),
            CHECK (
                previous_state IS NULL OR previous_state IN (
                    'created',
                    'waiting_for_sandbox',
                    'preparing_repository',
                    'loading_project_instructions',
                    'running_setup_scripts',
                    'setting_up_hooks',
                    'setting_up_skills_plugins',
                    'indexing_workspace',
                    'starting_conversation',
                    'ready',
                    'failed',
                    'paused',
                    'archived'
                )
            ),
            FOREIGN KEY (project_id, run_id)
                REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS agent_environment_pending_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id TEXT NOT NULL,
            run_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            submitted_at TEXT NOT NULL,
            delivered_at TEXT,
            CHECK (role IN ('user')),
            CHECK (content <> ''),
            CHECK (delivered_at IS NULL OR delivered_at <> ''),
            FOREIGN KEY (project_id, run_id)
                REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_agent_environment_pending_messages_run
            ON agent_environment_pending_messages(project_id, run_id, delivered_at, id ASC);
        "#,
    )?;

    if table_exists(transaction, "agent_events")?
        && !agent_events_supports_current_event_kinds(transaction)?
    {
        rebuild_agent_events_with_current_event_kind_constraint(
            transaction,
            "agent_events_before_environment_lifecycle",
        )?;
    }

    Ok(())
}

fn migrate_agent_events_current_event_kinds(
    transaction: &Transaction<'_>,
) -> rusqlite_migration::HookResult {
    if table_exists(transaction, "agent_events")?
        && !agent_events_supports_current_event_kinds(transaction)?
    {
        rebuild_agent_events_with_current_event_kind_constraint(
            transaction,
            "agent_events_before_current_event_kinds",
        )?;
    }

    Ok(())
}

fn table_exists(transaction: &Transaction<'_>, table: &str) -> rusqlite::Result<bool> {
    transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [table],
        |row| row.get::<_, bool>(0),
    )
}

fn agent_events_supports_current_event_kinds(
    transaction: &Transaction<'_>,
) -> rusqlite::Result<bool> {
    let sql = transaction.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'agent_events'",
        [],
        |row| row.get::<_, String>(0),
    )?;
    Ok(CURRENT_AGENT_EVENT_KIND_SQL_VALUES
        .iter()
        .all(|kind| sql.contains(&format!("'{kind}'"))))
}

const CURRENT_AGENT_EVENT_KIND_SQL_VALUES: &[&str] = &[
    "run_started",
    "message_delta",
    "reasoning_summary",
    "tool_started",
    "tool_delta",
    "tool_completed",
    "file_changed",
    "command_output",
    "validation_started",
    "validation_completed",
    "tool_registry_snapshot",
    "policy_decision",
    "state_transition",
    "plan_updated",
    "verification_gate",
    "context_manifest_recorded",
    "retrieval_performed",
    "memory_candidate_captured",
    "environment_lifecycle_update",
    "sandbox_lifecycle_update",
    "action_required",
    "approval_required",
    "tool_permission_grant",
    "provider_model_changed",
    "runtime_settings_changed",
    "run_paused",
    "run_completed",
    "run_failed",
];

fn rebuild_agent_events_with_current_event_kind_constraint(
    transaction: &Transaction<'_>,
    backup_table: &str,
) -> rusqlite::Result<()> {
    transaction.execute_batch(&format!(
        r#"
        ALTER TABLE agent_events RENAME TO {backup_table};

        CREATE TABLE agent_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id TEXT NOT NULL,
            run_id TEXT NOT NULL,
            event_kind TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            CHECK (event_kind IN (
                'run_started',
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
                'context_manifest_recorded',
                'retrieval_performed',
                'memory_candidate_captured',
                'environment_lifecycle_update',
                'sandbox_lifecycle_update',
                'action_required',
                'approval_required',
                'tool_permission_grant',
                'provider_model_changed',
                'runtime_settings_changed',
                'run_paused',
                'run_completed',
                'run_failed'
            )),
            CHECK (payload_json <> ''),
            CHECK (json_valid(payload_json)),
            FOREIGN KEY (project_id, run_id)
                REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
        );

        INSERT INTO agent_events (id, project_id, run_id, event_kind, payload_json, created_at)
        SELECT id, project_id, run_id, event_kind, payload_json, created_at
        FROM {backup_table};

        DROP TABLE {backup_table};

        CREATE INDEX IF NOT EXISTS idx_agent_events_project_run_id
            ON agent_events(project_id, run_id, id ASC);
        "#
    ))
}

fn add_column_if_missing(
    transaction: &Transaction<'_>,
    table: &str,
    column: &str,
    column_definition: &str,
) -> rusqlite::Result<()> {
    if table_has_column(transaction, table, column)? {
        return Ok(());
    }

    transaction.execute_batch(&format!(
        "ALTER TABLE {table} ADD COLUMN {column} {column_definition};"
    ))
}

fn table_has_column(
    transaction: &Transaction<'_>,
    table: &str,
    column: &str,
) -> rusqlite::Result<bool> {
    let mut statement = transaction.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for existing in columns {
        if existing? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

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
    fn event_kind_repair_migration_updates_already_migrated_agent_events_tables() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;

                CREATE TABLE agent_runs (
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    PRIMARY KEY (project_id, run_id)
                );

                CREATE TABLE agent_events (
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
                        'environment_lifecycle_update',
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

                INSERT INTO agent_runs (project_id, run_id)
                VALUES ('project-1', 'run-1');

                INSERT INTO agent_events (project_id, run_id, event_kind, payload_json, created_at)
                VALUES ('project-1', 'run-1', 'message_delta', '{}', '2026-05-05T16:32:47Z');
                "#,
            )
            .expect("create old constrained schema");

        let transaction = connection.transaction().expect("start transaction");
        migrate_agent_events_current_event_kinds(&transaction).expect("repair event kinds");
        transaction.commit().expect("commit repair");

        connection
            .execute(
                "INSERT INTO agent_events (project_id, run_id, event_kind, payload_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    "project-1",
                    "run-1",
                    "run_started",
                    "{}",
                    "2026-05-05T16:32:48Z"
                ],
            )
            .expect("insert run_started after repair");
        connection
            .execute(
                "INSERT INTO agent_events (project_id, run_id, event_kind, payload_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    "project-1",
                    "run-1",
                    "context_manifest_recorded",
                    "{}",
                    "2026-05-05T16:32:49Z"
                ],
            )
            .expect("insert context_manifest_recorded after repair");

        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM agent_events", [], |row| row.get(0))
            .expect("count events");
        assert_eq!(count, 3);
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
                "agent_capability_revocations",
                "agent_checkpoints",
                "agent_compactions",
                "agent_context_manifests",
                "agent_context_policy_settings",
                "agent_coordination_events",
                "agent_coordination_presence",
                "agent_definition_versions",
                "agent_definitions",
                "agent_embedding_backfill_jobs",
                "agent_environment_lifecycle_snapshots",
                "agent_environment_pending_messages",
                "agent_events",
                "agent_file_changes",
                "agent_file_reservations",
                "agent_handoff_lineage",
                "agent_mailbox_acknowledgements",
                "agent_mailbox_items",
                "agent_message_attachments",
                "agent_messages",
                "agent_plan_handoffs",
                "agent_plan_packs",
                "agent_plan_responses",
                "agent_plan_sessions",
                "agent_plan_slices",
                "agent_retrieval_queries",
                "agent_retrieval_results",
                "agent_runs",
                "agent_runtime_audit_events",
                "agent_session_lineage",
                "agent_sessions",
                "agent_subagent_tasks",
                "agent_tool_calls",
                "agent_usage",
                "autonomous_runs",
                "code_blobs",
                "code_change_groups",
                "code_commits",
                "code_file_versions",
                "code_history_operations",
                "code_patch_files",
                "code_patch_hunks",
                "code_patchsets",
                "code_path_epochs",
                "code_rollback_operations",
                "code_snapshots",
                "code_workspace_heads",
                "cross_store_outbox",
                "installed_plugin_records",
                "installed_skill_records",
                "meta",
                "notification_dispatches",
                "notification_reply_claims",
                "notification_routes",
                "operator_approvals",
                "operator_resume_history",
                "operator_verification_records",
                "project_storage_maintenance_runs",
                "projects",
                "repositories",
                "runtime_run_checkpoints",
                "runtime_runs",
                "runtime_sessions",
                "workspace_index_files",
                "workspace_index_metadata",
            ],
            "fresh project databases should start from the current baseline schema"
        );
    }

    #[test]
    fn agent_subagent_tasks_table_tracks_budget_and_resolution_state() {
        let connection = migrate_to_latest_in_memory();
        let columns = table_columns(&connection, "agent_subagent_tasks");
        for column in [
            "prompt_hash",
            "prompt_preview",
            "max_tool_calls",
            "max_tokens",
            "max_cost_micros",
            "used_tool_calls",
            "used_tokens",
            "used_cost_micros",
            "budget_status",
            "budget_diagnostic_json",
            "parent_decision",
            "latest_summary",
        ] {
            assert!(
                columns.contains(&column.to_string()),
                "agent_subagent_tasks should include {column}"
            );
        }
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
    fn baseline_seeds_test_agent_definition_registry_entry() {
        let connection = migrate_to_latest_in_memory();
        let built_ins = {
            let mut statement = connection
                .prepare(
                    r#"
                    SELECT
                        definition_id,
                        current_version,
                        display_name,
                        short_label,
                        scope,
                        lifecycle_state,
                        base_capability_profile
                    FROM agent_definitions
                    WHERE scope = 'built_in'
                    ORDER BY definition_id
                    "#,
                )
                .expect("prepare built-in definitions query");
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                })
                .expect("query built-in definitions");
            rows.collect::<Result<Vec<_>, _>>()
                .expect("collect built-in definitions")
        };

        assert_eq!(
            built_ins,
            vec![
                (
                    "agent_create".into(),
                    1,
                    "Agent Create".into(),
                    "Create".into(),
                    "built_in".into(),
                    "active".into(),
                    "agent_builder".into(),
                ),
                (
                    "ask".into(),
                    1,
                    "Ask".into(),
                    "Ask".into(),
                    "built_in".into(),
                    "active".into(),
                    "observe_only".into(),
                ),
                (
                    "crawl".into(),
                    1,
                    "Crawl".into(),
                    "Crawl".into(),
                    "built_in".into(),
                    "active".into(),
                    "repository_recon".into(),
                ),
                (
                    "debug".into(),
                    1,
                    "Debug".into(),
                    "Debug".into(),
                    "built_in".into(),
                    "active".into(),
                    "debugging".into(),
                ),
                (
                    "engineer".into(),
                    1,
                    "Engineer".into(),
                    "Build".into(),
                    "built_in".into(),
                    "active".into(),
                    "engineering".into(),
                ),
                (
                    "plan".into(),
                    1,
                    "Plan".into(),
                    "Plan".into(),
                    "built_in".into(),
                    "active".into(),
                    "planning".into(),
                ),
                (
                    "test".into(),
                    1,
                    "Test".into(),
                    "Test".into(),
                    "built_in".into(),
                    "active".into(),
                    "harness_test".into(),
                ),
            ],
            "fresh project databases should seed every built-in agent definition"
        );

        let test_snapshot = connection
            .query_row(
                r#"
                SELECT
                    json_extract(snapshot_json, '$.id'),
                    json_extract(snapshot_json, '$.displayName'),
                    json_extract(snapshot_json, '$.scope'),
                    json_extract(snapshot_json, '$.baseCapabilityProfile'),
                    json_extract(snapshot_json, '$.defaultApprovalMode'),
                    json_extract(snapshot_json, '$.promptPolicy'),
                    json_extract(snapshot_json, '$.toolPolicy'),
                    json_extract(snapshot_json, '$.outputContract'),
                    json_extract(validation_report_json, '$.source')
                FROM agent_definition_versions
                WHERE definition_id = 'test'
                  AND version = 1
                "#,
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                },
            )
            .expect("load test agent definition version snapshot");

        assert_eq!(
            test_snapshot,
            (
                "test".into(),
                "Test".into(),
                "built_in".into(),
                "harness_test".into(),
                "suggest".into(),
                "harness_test".into(),
                "harness_test".into(),
                "harness_test_report".into(),
                "seed".into(),
            )
        );

        let plan_snapshot = connection
            .query_row(
                r#"
                SELECT
                    json_extract(snapshot_json, '$.id'),
                    json_extract(snapshot_json, '$.displayName'),
                    json_extract(snapshot_json, '$.scope'),
                    json_extract(snapshot_json, '$.baseCapabilityProfile'),
                    json_extract(snapshot_json, '$.defaultApprovalMode'),
                    json_extract(snapshot_json, '$.promptPolicy'),
                    json_extract(snapshot_json, '$.toolPolicy'),
                    json_extract(snapshot_json, '$.outputContract'),
                    json_extract(validation_report_json, '$.source')
                FROM agent_definition_versions
                WHERE definition_id = 'plan'
                  AND version = 1
                "#,
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                },
            )
            .expect("load plan agent definition version snapshot");

        assert_eq!(
            plan_snapshot,
            (
                "plan".into(),
                "Plan".into(),
                "built_in".into(),
                "planning".into(),
                "suggest".into(),
                "plan".into(),
                "planning".into(),
                "plan_pack".into(),
                "seed".into(),
            )
        );
    }

    #[test]
    fn plan_pack_schema_uses_app_data_tables() {
        let connection = migrate_to_latest_in_memory();

        assert_eq!(
            table_columns(&connection, "agent_plan_packs"),
            vec![
                "project_id",
                "plan_id",
                "plan_session_id",
                "schema_name",
                "status",
                "agent_session_id",
                "source_run_id",
                "source_agent_session_id",
                "title",
                "goal",
                "markdown",
                "pack_json",
                "accepted_revision",
                "lance_record_id",
                "supersedes_plan_id",
                "superseded_by_plan_id",
                "created_at",
                "updated_at",
                "accepted_at",
            ],
            "accepted Plan Packs should persist in app-data SQLite, not repo-local state"
        );
        assert_eq!(
            table_columns(&connection, "agent_plan_responses"),
            vec![
                "project_id",
                "plan_session_id",
                "action_id",
                "question_id",
                "answer_shape",
                "raw_answer",
                "normalized_answer_json",
                "created_at",
                "resolved_at",
            ],
            "structured planning responses should be linked to the planning session"
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
                    trace_id,
                    provider_id,
                    model_id,
                    status,
                    prompt,
                    system_prompt,
                    started_at,
                    updated_at
                )
                VALUES ('ask', 'ask', 1, ?1, ?2, ?3, ?4, 'fake_provider', 'fake-model', 'running', 'prompt', 'system', ?5, ?5)
                "#,
                params![
                    "project-1",
                    "agent-session-main",
                    "run-1",
                    "0123456789abcdef0123456789abcdef",
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
    fn v15_project_state_repairs_missing_agent_file_change_group_id() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;

                CREATE TABLE agent_runs (
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    trace_id TEXT NOT NULL,
                    lineage_kind TEXT NOT NULL,
                    parent_run_id TEXT,
                    parent_trace_id TEXT,
                    parent_subagent_id TEXT,
                    subagent_role TEXT,
                    PRIMARY KEY (project_id, run_id)
                );

                CREATE TABLE agent_file_changes (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    trace_id TEXT NOT NULL,
                    top_level_run_id TEXT NOT NULL,
                    subagent_id TEXT,
                    subagent_role TEXT,
                    path TEXT NOT NULL,
                    operation TEXT NOT NULL,
                    old_hash TEXT,
                    new_hash TEXT,
                    created_at TEXT NOT NULL
                );

                INSERT INTO agent_runs (
                    project_id,
                    run_id,
                    trace_id,
                    lineage_kind
                )
                VALUES (
                    'project-v15',
                    'run-v15',
                    '0123456789abcdef0123456789abcdef',
                    'top_level'
                );

                INSERT INTO agent_file_changes (
                    project_id,
                    run_id,
                    trace_id,
                    top_level_run_id,
                    path,
                    operation,
                    created_at
                )
                VALUES (
                    'project-v15',
                    'run-v15',
                    '0123456789abcdef0123456789abcdef',
                    'run-v15',
                    'src/lib.rs',
                    'edit',
                    '2026-05-07T12:34:00Z'
                );

                PRAGMA user_version = 15;
                "#,
            )
            .expect("seed v15 schema missing change_group_id");

        migrations()
            .to_latest(&mut connection)
            .expect("repair v15 schema");

        let file_change_columns = table_columns(&connection, "agent_file_changes");
        assert!(
            file_change_columns.contains(&"change_group_id".to_string()),
            "agent_file_changes should include change_group_id after repair"
        );

        connection
            .prepare(
                "SELECT id, project_id, run_id, trace_id, top_level_run_id, subagent_id, subagent_role, change_group_id
                 FROM agent_file_changes",
            )
            .expect("owned-agent file change query prepares after repair");
    }

    #[test]
    fn legacy_v5_project_state_migrates_agent_trace_columns() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;

                CREATE TABLE agent_runs (
                    runtime_agent_id TEXT NOT NULL,
                    agent_definition_id TEXT NOT NULL,
                    agent_definition_version INTEGER NOT NULL,
                    project_id TEXT NOT NULL,
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
                    PRIMARY KEY (project_id, run_id)
                );

                CREATE TABLE agent_file_changes (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    path TEXT NOT NULL,
                    operation TEXT NOT NULL,
                    old_hash TEXT,
                    new_hash TEXT,
                    created_at TEXT NOT NULL
                );

                CREATE TABLE agent_coordination_presence (
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    trace_id TEXT NOT NULL
                );

                CREATE TABLE agent_file_reservations (
                    project_id TEXT NOT NULL
                );

                CREATE TABLE projects (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL
                );

                INSERT INTO projects (id, name)
                VALUES ('project-legacy', 'Legacy project');

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
                VALUES (
                    'ask',
                    'ask',
                    1,
                    'project-legacy',
                    'agent-session-legacy',
                    'run-legacy',
                    'openai_codex',
                    'gpt-5.4',
                    'completed',
                    'prompt',
                    'system',
                    '2026-05-01T12:00:00Z',
                    '2026-05-01T12:01:00Z'
                );

                INSERT INTO agent_file_changes (
                    project_id,
                    run_id,
                    path,
                    operation,
                    created_at
                )
                VALUES (
                    'project-legacy',
                    'run-legacy',
                    'src/lib.rs',
                    'edit',
                    '2026-05-01T12:02:00Z'
                );

                INSERT INTO agent_coordination_presence (
                    project_id,
                    run_id,
                    trace_id
                )
                VALUES (
                    'project-legacy',
                    'run-legacy',
                    '0123456789abcdef0123456789abcdef'
                );

                PRAGMA user_version = 5;
                "#,
            )
            .expect("seed legacy v5 schema");

        migrations()
            .to_latest(&mut connection)
            .expect("migrate legacy v5 schema");

        let run_columns = table_columns(&connection, "agent_runs");
        for column in [
            "trace_id",
            "lineage_kind",
            "parent_run_id",
            "parent_trace_id",
            "parent_subagent_id",
            "subagent_role",
        ] {
            assert!(
                run_columns.contains(&column.to_string()),
                "agent_runs should include {column}"
            );
        }

        let file_change_columns = table_columns(&connection, "agent_file_changes");
        for column in [
            "trace_id",
            "top_level_run_id",
            "subagent_id",
            "subagent_role",
            "change_group_id",
        ] {
            assert!(
                file_change_columns.contains(&column.to_string()),
                "agent_file_changes should include {column}"
            );
        }

        let (trace_id, lineage_kind): (String, String) = connection
            .query_row(
                "SELECT trace_id, lineage_kind FROM agent_runs WHERE project_id = 'project-legacy' AND run_id = 'run-legacy'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read migrated agent run");
        assert_eq!(trace_id, "0123456789abcdef0123456789abcdef");
        assert_eq!(lineage_kind, "top_level");

        let (change_trace_id, top_level_run_id): (String, String) = connection
            .query_row(
                "SELECT trace_id, top_level_run_id FROM agent_file_changes WHERE project_id = 'project-legacy' AND run_id = 'run-legacy'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read migrated file change");
        assert_eq!(change_trace_id, trace_id);
        assert_eq!(top_level_run_id, "run-legacy");
    }

    #[test]
    fn current_app_data_project_state_repairs_agent_message_provider_metadata_column() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;

                CREATE TABLE agent_messages (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    content TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );

                INSERT INTO agent_messages (
                    project_id,
                    run_id,
                    role,
                    content,
                    created_at
                )
                VALUES (
                    'project-v19',
                    'run-v19',
                    'user',
                    'hello',
                    '2026-05-09T14:00:00Z'
                );

                PRAGMA user_version = 19;
                "#,
            )
            .expect("seed current app-data schema missing provider metadata column");

        migrations()
            .to_latest(&mut connection)
            .expect("repair current app-data schema");

        let message_columns = table_columns(&connection, "agent_messages");
        assert!(
            message_columns.contains(&"provider_metadata_json".to_string()),
            "agent_messages should include provider_metadata_json after repair"
        );

        connection
            .prepare(
                "SELECT id, project_id, run_id, role, content, provider_metadata_json, created_at
                 FROM agent_messages
                 WHERE project_id = ?1
                   AND run_id = ?2
                 ORDER BY id ASC",
            )
            .expect("owned-agent message query prepares after repair");
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
