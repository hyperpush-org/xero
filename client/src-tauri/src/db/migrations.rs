use std::sync::LazyLock;

use rusqlite_migration::{Migrations, M};

pub const PROJECT_DATABASE_SCHEMA_VERSION: i64 = 35;

pub fn migrations() -> &'static Migrations<'static> {
    static MIGRATIONS: LazyLock<Migrations<'static>> = LazyLock::new(|| {
        Migrations::new(vec![
            M::up(BASELINE_SCHEMA_SQL),
            M::up(MIGRATION_001_AGENT_MESSAGE_ATTACHMENTS_SQL),
            M::up(MIGRATION_002_WORKSPACE_INDEX_SQL),
            M::up(MIGRATION_003_AGENT_COORDINATION_SQL),
            M::up(MIGRATION_004_AGENT_MAILBOX_SQL),
            M::up(MIGRATION_005_AGENT_PLAN_PACKS_SQL),
            M::up(NOOP_SCHEMA_VERSION_MARKER_SQL),
            M::up(NOOP_SCHEMA_VERSION_MARKER_SQL),
            M::up(NOOP_SCHEMA_VERSION_MARKER_SQL),
            M::up(MIGRATION_009_PROJECT_ORIGIN_SQL),
            M::up(MIGRATION_010_CODE_ROLLBACK_STORAGE_SQL),
            M::up(MIGRATION_011_CODE_HISTORY_WORKSPACE_HEAD_SQL),
            M::up(MIGRATION_012_CODE_HISTORY_COMMIT_PATCHSET_SQL),
            M::up(MIGRATION_013_CODE_HISTORY_OPERATIONS_SQL),
            M::up(MIGRATION_014_AGENT_RESERVATION_INVALIDATIONS_SQL),
            M::up(NOOP_SCHEMA_VERSION_MARKER_SQL),
            M::up(MIGRATION_015_CROSS_STORE_OUTBOX_SQL),
            M::up(MIGRATION_016_PROJECT_STORAGE_MAINTENANCE_SQL),
            M::up(MIGRATION_017_AGENT_AUDIT_AND_REVOCATION_SQL),
            M::up(NOOP_SCHEMA_VERSION_MARKER_SQL),
            M::up(MIGRATION_018_AGENT_SUBAGENT_TASKS_SQL),
            M::up(NOOP_SCHEMA_VERSION_MARKER_SQL),
            M::up(NOOP_SCHEMA_VERSION_MARKER_SQL),
            M::up(MIGRATION_019_RUNTIME_ATTACHED_SKILL_SNAPSHOTS_SQL),
            M::up(BUILTIN_AGENT_WORKFLOW_STRUCTURE_INSERT_SQL),
            M::up(MIGRATION_020_PROJECT_START_TARGETS_SQL),
            M::up(NOOP_SCHEMA_VERSION_MARKER_SQL),
            M::up(NOOP_SCHEMA_VERSION_MARKER_SQL),
            M::up(MIGRATION_024_AGENT_SESSION_REMOTE_VISIBILITY_SQL),
            M::up(NOOP_SCHEMA_VERSION_MARKER_SQL),
            M::up(NOOP_SCHEMA_VERSION_MARKER_SQL),
            M::up(MIGRATION_025_MULTI_AGENT_WORKFLOWS_SQL),
            M::up(MIGRATION_026_AGENT_CREATE_WORKFLOW_DEFINITIONS_SQL),
            M::up(MIGRATION_027_DELIVERY_STATE_SQL),
            M::up(MIGRATION_028_COMPUTER_USE_MODE_SQL),
        ])
    });

    &MIGRATIONS
}

const NOOP_SCHEMA_VERSION_MARKER_SQL: &str = "";

const MIGRATION_028_COMPUTER_USE_MODE_SQL: &str = r#"
    ALTER TABLE agent_sessions ADD COLUMN session_kind TEXT NOT NULL DEFAULT 'standard' CHECK (session_kind IN ('standard', 'computer_use'));

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
        ('computer_use', 1, 'Computer Use', 'Computer', 'Follow direct user instructions by observing and controlling the local computer through bounded automation tools.', 'built_in', 'active', 'computer_use', '2026-05-24T00:00:00Z');

    INSERT OR IGNORE INTO agent_definition_versions (
        definition_id,
        version,
        snapshot_json,
        validation_report_json,
        created_at
    )
    VALUES
        ('computer_use', 1, '{"schema":"xero.agent_definition.v1","id":"computer_use","version":1,"displayName":"Computer Use","shortLabel":"Computer","description":"Follow direct user instructions by observing and controlling the local computer through bounded automation tools.","taskPurpose":"Observe visible computer state, perform bounded UI automation, ask before risky actions, and stop immediately when cancelled.","scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"computer_use","defaultApprovalMode":"suggest","allowedApprovalModes":["suggest"],"promptPolicy":"computer_use","toolPolicy":{"allowedTools":["tool_access","tool_search","todo","project_context_search","project_context_get","browser_observe","browser_control","browser","emulator","macos_automation","system_diagnostics_observe"],"deniedTools":["write","edit","patch","copy","fs_transaction","json_edit","toml_edit","yaml_edit","delete","rename","mkdir","command","command_run","command_session","command_session_start","command_session_read","command_session_stop","powershell","process_manager","git_status","git_diff","mcp","mcp_call_tool","skill","subagent"],"allowedEffectClasses":[],"browserControlAllowed":true,"commandAllowed":true,"destructiveWriteAllowed":false,"externalServiceAllowed":false,"skillRuntimeAllowed":false,"subagentAllowed":false},"outputContract":"answer","workflowContract":"","finalResponseContract":"Answer directly with what was done, what still needs user confirmation, or why the requested action was stopped. Do not include secrets.","projectDataPolicy":{"required":true,"recordKinds":["agent_handoff","project_fact","decision","constraint","question","context_note","diagnostic"],"structuredSchemas":["xero.project_record.v1"],"unstructuredScopes":["answer_note","session_summary","troubleshooting_note"],"memoryCandidateKinds":["project_fact","user_preference","decision","session_summary","troubleshooting"]},"attachedSkills":[]}', '{"status":"valid","source":"seed"}', '2026-05-24T00:00:00Z');
"#;

const MIGRATION_027_DELIVERY_STATE_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS delivery_projects (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        title TEXT NOT NULL,
        summary TEXT NOT NULL DEFAULT '',
        status TEXT NOT NULL DEFAULT 'active',
        metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (project_id, id),
        CHECK (id <> ''),
        CHECK (title <> ''),
        CHECK (status IN ('active', 'paused', 'completed', 'archived')),
        CHECK (created_at <> ''),
        CHECK (updated_at <> '')
    );

    CREATE TABLE IF NOT EXISTS delivery_milestones (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        delivery_project_id TEXT,
        title TEXT NOT NULL,
        summary TEXT NOT NULL DEFAULT '',
        goal TEXT NOT NULL DEFAULT '',
        status TEXT NOT NULL DEFAULT 'active',
        metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        completed_at TEXT,
        archived_at TEXT,
        PRIMARY KEY (project_id, id),
        CHECK (id <> ''),
        CHECK (title <> ''),
        CHECK (status IN ('active', 'paused', 'completed', 'archived')),
        FOREIGN KEY (project_id, delivery_project_id)
            REFERENCES delivery_projects(project_id, id) ON DELETE SET NULL
    );

    CREATE TABLE IF NOT EXISTS delivery_requirements (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        milestone_id TEXT NOT NULL,
        title TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        status TEXT NOT NULL DEFAULT 'open',
        priority INTEGER NOT NULL DEFAULT 0,
        metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (project_id, id),
        CHECK (id <> ''),
        CHECK (title <> ''),
        CHECK (status IN ('open', 'satisfied', 'deferred', 'archived')),
        FOREIGN KEY (project_id, milestone_id)
            REFERENCES delivery_milestones(project_id, id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS delivery_phases (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        milestone_id TEXT NOT NULL,
        phase_key TEXT NOT NULL,
        title TEXT NOT NULL,
        summary TEXT NOT NULL DEFAULT '',
        status TEXT NOT NULL DEFAULT 'incomplete',
        sort_order REAL NOT NULL DEFAULT 0,
        inserted_after_phase_id TEXT,
        metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        completed_at TEXT,
        PRIMARY KEY (project_id, id),
        UNIQUE (project_id, milestone_id, phase_key),
        CHECK (id <> ''),
        CHECK (phase_key <> ''),
        CHECK (title <> ''),
        CHECK (status IN ('incomplete', 'in_progress', 'blocked', 'complete', 'deferred', 'archived')),
        FOREIGN KEY (project_id, milestone_id)
            REFERENCES delivery_milestones(project_id, id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, inserted_after_phase_id)
            REFERENCES delivery_phases(project_id, id) ON DELETE SET NULL
    );

    CREATE TABLE IF NOT EXISTS delivery_phase_context (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        phase_id TEXT NOT NULL,
        context_json TEXT NOT NULL CHECK (json_valid(context_json)),
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (project_id, id),
        FOREIGN KEY (project_id, phase_id)
            REFERENCES delivery_phases(project_id, id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS delivery_phase_plans (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        phase_id TEXT NOT NULL,
        plan_json TEXT NOT NULL CHECK (json_valid(plan_json)),
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (project_id, id),
        FOREIGN KEY (project_id, phase_id)
            REFERENCES delivery_phases(project_id, id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS delivery_phase_summaries (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        phase_id TEXT NOT NULL,
        summary_json TEXT NOT NULL CHECK (json_valid(summary_json)),
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (project_id, id),
        FOREIGN KEY (project_id, phase_id)
            REFERENCES delivery_phases(project_id, id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS delivery_verification_evidence (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        phase_id TEXT,
        requirement_id TEXT,
        status TEXT NOT NULL DEFAULT 'recorded',
        evidence_json TEXT NOT NULL CHECK (json_valid(evidence_json)),
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (project_id, id),
        CHECK (status IN ('recorded', 'passed', 'failed', 'superseded')),
        FOREIGN KEY (project_id, phase_id)
            REFERENCES delivery_phases(project_id, id) ON DELETE SET NULL,
        FOREIGN KEY (project_id, requirement_id)
            REFERENCES delivery_requirements(project_id, id) ON DELETE SET NULL
    );

    CREATE TABLE IF NOT EXISTS delivery_deferred_items (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        milestone_id TEXT,
        phase_id TEXT,
        title TEXT NOT NULL,
        reason TEXT NOT NULL DEFAULT '',
        status TEXT NOT NULL DEFAULT 'open',
        metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (project_id, id),
        CHECK (title <> ''),
        CHECK (status IN ('open', 'accepted', 'closed', 'archived')),
        FOREIGN KEY (project_id, milestone_id)
            REFERENCES delivery_milestones(project_id, id) ON DELETE SET NULL,
        FOREIGN KEY (project_id, phase_id)
            REFERENCES delivery_phases(project_id, id) ON DELETE SET NULL
    );

    CREATE TABLE IF NOT EXISTS delivery_milestone_archives (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        milestone_id TEXT NOT NULL,
        archive_json TEXT NOT NULL CHECK (json_valid(archive_json)),
        created_at TEXT NOT NULL,
        PRIMARY KEY (project_id, id),
        FOREIGN KEY (project_id, milestone_id)
            REFERENCES delivery_milestones(project_id, id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS delivery_state_events (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        workflow_run_id TEXT,
        node_run_id TEXT,
        entity_type TEXT NOT NULL,
        entity_id TEXT NOT NULL,
        event_type TEXT NOT NULL,
        before_json TEXT CHECK (before_json IS NULL OR json_valid(before_json)),
        after_json TEXT CHECK (after_json IS NULL OR json_valid(after_json)),
        created_at TEXT NOT NULL,
        PRIMARY KEY (project_id, id),
        CHECK (entity_type <> ''),
        CHECK (entity_id <> ''),
        CHECK (event_type <> ''),
        FOREIGN KEY (project_id, workflow_run_id)
            REFERENCES workflow_runs(project_id, id) ON DELETE SET NULL,
        FOREIGN KEY (project_id, node_run_id)
            REFERENCES workflow_run_nodes(project_id, id) ON DELETE SET NULL
    );

    CREATE INDEX IF NOT EXISTS idx_delivery_milestones_project_status
        ON delivery_milestones(project_id, status, updated_at DESC);
    CREATE INDEX IF NOT EXISTS idx_delivery_phases_milestone_status
        ON delivery_phases(project_id, milestone_id, status, sort_order ASC);
    CREATE INDEX IF NOT EXISTS idx_delivery_requirements_milestone_status
        ON delivery_requirements(project_id, milestone_id, status);
    CREATE INDEX IF NOT EXISTS idx_delivery_state_events_project_created
        ON delivery_state_events(project_id, created_at ASC);
"#;

const MIGRATION_026_AGENT_CREATE_WORKFLOW_DEFINITIONS_SQL: &str = r#"
    INSERT OR IGNORE INTO agent_definition_versions (
        definition_id,
        version,
        snapshot_json,
        validation_report_json,
        created_at
    )
    VALUES
        ('agent_create', 3, '{"schema":"xero.agent_definition.v1","id":"agent_create","version":3,"displayName":"Agent Create","shortLabel":"Create","description":"Interview the user and draft high-quality custom agent or Workflow definitions.","taskPurpose":"Interview the user for agent or Workflow purpose, scope, risk tolerance, participating agents, and expected outputs, then draft schema-validated definitions.","scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"agent_builder","defaultApprovalMode":"suggest","allowedApprovalModes":["suggest"],"promptPolicy":"agent_create","toolPolicy":"agent_builder","workflowContract":"Complete the interview before drafting. Close the `interview_complete` todo to enter the drafting phase.","finalResponseContract":"Present a reviewable agent or Workflow definition draft with validation diagnostics.","workflowStructure":{"startPhaseId":"interview","phases":[{"id":"interview","title":"Interview","description":"Clarify purpose, scope, risk tolerance, required agents or Workflow nodes, and example tasks. Close the `interview_complete` todo when the interview is finished.","allowedTools":["read","search","find","tool_access","tool_search","todo"],"requiredChecks":[{"kind":"todo_completed","todoId":"interview_complete","description":"Close the `interview_complete` todo when the interview is finished."}]},{"id":"draft","title":"Draft","description":"Validate and save a draft definition through `agent_definition` or `workflow_definition`.","allowedTools":["read","search","find","tool_access","tool_search","agent_definition","workflow_definition","todo"]}]},"attachedSkills":[]}', '{"status":"valid","source":"seed"}', '2026-05-24T00:00:00Z');

    UPDATE agent_definitions
    SET current_version = 3,
        description = 'Interview the user and draft high-quality custom agent or Workflow definitions.',
        updated_at = '2026-05-24T00:00:00Z'
    WHERE definition_id = 'agent_create'
      AND current_version < 3;
"#;

const MIGRATION_025_MULTI_AGENT_WORKFLOWS_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS workflow_definitions (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        active_version_id TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (project_id, id),
        CHECK (id <> ''),
        CHECK (name <> ''),
        CHECK (active_version_id <> ''),
        CHECK (created_at <> ''),
        CHECK (updated_at <> '')
    );

    CREATE TABLE IF NOT EXISTS workflow_definition_versions (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL,
        workflow_id TEXT NOT NULL,
        version_number INTEGER NOT NULL CHECK (version_number > 0),
        definition_json TEXT NOT NULL CHECK (definition_json <> '' AND json_valid(definition_json)),
        created_at TEXT NOT NULL,
        PRIMARY KEY (project_id, id),
        UNIQUE (project_id, workflow_id, version_number),
        CHECK (id <> ''),
        CHECK (workflow_id <> ''),
        CHECK (created_at <> ''),
        FOREIGN KEY (project_id, workflow_id)
            REFERENCES workflow_definitions(project_id, id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_workflow_definition_versions_workflow
        ON workflow_definition_versions(project_id, workflow_id, version_number DESC);

    CREATE TABLE IF NOT EXISTS workflow_runs (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        workflow_id TEXT NOT NULL,
        workflow_version_id TEXT NOT NULL,
        workflow_version_number INTEGER NOT NULL CHECK (workflow_version_number > 0),
        status TEXT NOT NULL,
        terminal_status TEXT,
        definition_json TEXT NOT NULL CHECK (definition_json <> '' AND json_valid(definition_json)),
        initial_input_json TEXT CHECK (initial_input_json IS NULL OR json_valid(initial_input_json)),
        started_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        completed_at TEXT,
        cancellation_reason TEXT,
        PRIMARY KEY (project_id, id),
        CHECK (id <> ''),
        CHECK (workflow_id <> ''),
        CHECK (workflow_version_id <> ''),
        CHECK (status IN ('queued', 'running', 'paused', 'completed', 'failed', 'cancelled')),
        CHECK (terminal_status IS NULL OR terminal_status IN ('success', 'failure', 'cancelled', 'needs_human')),
        CHECK (started_at <> ''),
        CHECK (updated_at <> ''),
        CHECK (completed_at IS NULL OR completed_at <> ''),
        FOREIGN KEY (project_id, workflow_version_id)
            REFERENCES workflow_definition_versions(project_id, id)
    );

    CREATE INDEX IF NOT EXISTS idx_workflow_runs_project_updated
        ON workflow_runs(project_id, updated_at DESC, started_at DESC);
    CREATE INDEX IF NOT EXISTS idx_workflow_runs_workflow_updated
        ON workflow_runs(project_id, workflow_id, updated_at DESC);
    CREATE INDEX IF NOT EXISTS idx_workflow_runs_status
        ON workflow_runs(project_id, status, updated_at DESC);

    CREATE TABLE IF NOT EXISTS workflow_run_nodes (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL,
        workflow_run_id TEXT NOT NULL,
        node_id TEXT NOT NULL,
        node_type TEXT NOT NULL,
        status TEXT NOT NULL,
        attempt_number INTEGER NOT NULL CHECK (attempt_number >= 0),
        runtime_run_id TEXT,
        agent_session_id TEXT,
        failure_class TEXT,
        started_at TEXT,
        updated_at TEXT NOT NULL,
        completed_at TEXT,
        idempotency_key TEXT NOT NULL,
        PRIMARY KEY (project_id, id),
        UNIQUE (project_id, workflow_run_id, node_id, attempt_number),
        UNIQUE (project_id, idempotency_key),
        CHECK (id <> ''),
        CHECK (workflow_run_id <> ''),
        CHECK (node_id <> ''),
        CHECK (node_type IN ('agent', 'router', 'gate', 'human_checkpoint', 'merge', 'terminal', 'state_read', 'state_write', 'state_patch', 'state_query', 'state_checkpoint', 'collection_loop', 'subgraph', 'command')),
        CHECK (status IN ('pending', 'eligible', 'starting', 'running', 'waiting_on_gate', 'succeeded', 'failed', 'stalled', 'skipped', 'cancelled')),
        CHECK (runtime_run_id IS NULL OR runtime_run_id <> ''),
        CHECK (agent_session_id IS NULL OR agent_session_id <> ''),
        CHECK (failure_class IS NULL OR failure_class <> ''),
        CHECK (started_at IS NULL OR started_at <> ''),
        CHECK (updated_at <> ''),
        CHECK (completed_at IS NULL OR completed_at <> ''),
        CHECK (idempotency_key <> ''),
        FOREIGN KEY (project_id, workflow_run_id)
            REFERENCES workflow_runs(project_id, id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, runtime_run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE SET NULL,
        FOREIGN KEY (project_id, agent_session_id)
            REFERENCES agent_sessions(project_id, agent_session_id) ON DELETE SET NULL
    );

    CREATE INDEX IF NOT EXISTS idx_workflow_run_nodes_run_status
        ON workflow_run_nodes(project_id, workflow_run_id, status, updated_at DESC);
    CREATE INDEX IF NOT EXISTS idx_workflow_run_nodes_runtime
        ON workflow_run_nodes(project_id, runtime_run_id);

    CREATE TABLE IF NOT EXISTS workflow_run_edges (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL,
        workflow_run_id TEXT NOT NULL,
        from_node_id TEXT NOT NULL,
        to_node_id TEXT NOT NULL,
        edge_id TEXT NOT NULL,
        matched INTEGER NOT NULL DEFAULT 1 CHECK (matched IN (0, 1)),
        condition_json TEXT NOT NULL CHECK (condition_json <> '' AND json_valid(condition_json)),
        evidence_json TEXT NOT NULL CHECK (evidence_json <> '' AND json_valid(evidence_json)),
        created_at TEXT NOT NULL,
        PRIMARY KEY (project_id, id),
        CHECK (workflow_run_id <> ''),
        CHECK (from_node_id <> ''),
        CHECK (to_node_id <> ''),
        CHECK (edge_id <> ''),
        CHECK (created_at <> ''),
        FOREIGN KEY (project_id, workflow_run_id)
            REFERENCES workflow_runs(project_id, id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_workflow_run_edges_run_created
        ON workflow_run_edges(project_id, workflow_run_id, created_at ASC);

    CREATE TABLE IF NOT EXISTS workflow_artifacts (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL,
        workflow_run_id TEXT NOT NULL,
        producer_node_run_id TEXT NOT NULL,
        artifact_type TEXT NOT NULL,
        schema_version INTEGER NOT NULL CHECK (schema_version > 0),
        payload_json TEXT NOT NULL CHECK (payload_json <> '' AND json_valid(payload_json)),
        render_text TEXT,
        created_at TEXT NOT NULL,
        PRIMARY KEY (project_id, id),
        CHECK (workflow_run_id <> ''),
        CHECK (producer_node_run_id <> ''),
        CHECK (artifact_type <> ''),
        CHECK (created_at <> ''),
        FOREIGN KEY (project_id, workflow_run_id)
            REFERENCES workflow_runs(project_id, id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, producer_node_run_id)
            REFERENCES workflow_run_nodes(project_id, id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_workflow_artifacts_run_type
        ON workflow_artifacts(project_id, workflow_run_id, artifact_type, created_at ASC);
    CREATE INDEX IF NOT EXISTS idx_workflow_artifacts_producer
        ON workflow_artifacts(project_id, producer_node_run_id);

    CREATE TABLE IF NOT EXISTS workflow_gate_decisions (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL,
        workflow_run_id TEXT NOT NULL,
        node_run_id TEXT NOT NULL,
        checkpoint_type TEXT NOT NULL,
        decision TEXT NOT NULL,
        decision_payload_json TEXT CHECK (decision_payload_json IS NULL OR json_valid(decision_payload_json)),
        decided_at TEXT NOT NULL,
        PRIMARY KEY (project_id, id),
        CHECK (workflow_run_id <> ''),
        CHECK (node_run_id <> ''),
        CHECK (checkpoint_type IN ('human_verify', 'decision', 'human_action')),
        CHECK (decision <> ''),
        CHECK (decided_at <> ''),
        FOREIGN KEY (project_id, workflow_run_id)
            REFERENCES workflow_runs(project_id, id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, node_run_id)
            REFERENCES workflow_run_nodes(project_id, id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_workflow_gate_decisions_run
        ON workflow_gate_decisions(project_id, workflow_run_id, decided_at ASC);

    CREATE TABLE IF NOT EXISTS workflow_loop_attempts (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL,
        workflow_run_id TEXT NOT NULL,
        loop_key TEXT NOT NULL,
        attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
        last_node_run_id TEXT,
        exhausted INTEGER NOT NULL DEFAULT 0 CHECK (exhausted IN (0, 1)),
        updated_at TEXT NOT NULL,
        PRIMARY KEY (project_id, id),
        UNIQUE (project_id, workflow_run_id, loop_key),
        CHECK (workflow_run_id <> ''),
        CHECK (loop_key <> ''),
        CHECK (last_node_run_id IS NULL OR last_node_run_id <> ''),
        CHECK (updated_at <> ''),
        FOREIGN KEY (project_id, workflow_run_id)
            REFERENCES workflow_runs(project_id, id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, last_node_run_id)
            REFERENCES workflow_run_nodes(project_id, id) ON DELETE SET NULL
    );

    CREATE INDEX IF NOT EXISTS idx_workflow_loop_attempts_run
        ON workflow_loop_attempts(project_id, workflow_run_id);

    CREATE TABLE IF NOT EXISTS workflow_events (
        id TEXT NOT NULL,
        project_id TEXT NOT NULL,
        workflow_run_id TEXT NOT NULL,
        node_run_id TEXT,
        event_type TEXT NOT NULL,
        event_json TEXT NOT NULL CHECK (event_json <> '' AND json_valid(event_json)),
        created_at TEXT NOT NULL,
        PRIMARY KEY (project_id, id),
        CHECK (workflow_run_id <> ''),
        CHECK (node_run_id IS NULL OR node_run_id <> ''),
        CHECK (event_type <> ''),
        CHECK (created_at <> ''),
        FOREIGN KEY (project_id, workflow_run_id)
            REFERENCES workflow_runs(project_id, id) ON DELETE CASCADE,
        FOREIGN KEY (project_id, node_run_id)
            REFERENCES workflow_run_nodes(project_id, id) ON DELETE SET NULL
    );

    CREATE INDEX IF NOT EXISTS idx_workflow_events_run_created
        ON workflow_events(project_id, workflow_run_id, created_at ASC);
"#;

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

const MIGRATION_019_RUNTIME_ATTACHED_SKILL_SNAPSHOTS_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS runtime_run_attached_skill_snapshots (
        project_id TEXT NOT NULL,
        run_id TEXT NOT NULL,
        snapshot_json TEXT NOT NULL,
        resolved_at TEXT NOT NULL,
        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        PRIMARY KEY (project_id, run_id),
        CHECK (project_id <> ''),
        CHECK (run_id <> ''),
        CHECK (snapshot_json <> '' AND json_valid(snapshot_json)),
        CHECK (resolved_at <> ''),
        CHECK (updated_at <> ''),
        FOREIGN KEY (project_id, run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_runtime_run_attached_skill_snapshots_updated
        ON runtime_run_attached_skill_snapshots(project_id, updated_at DESC);
"#;

const MIGRATION_009_PROJECT_ORIGIN_SQL: &str = r#"
    ALTER TABLE projects ADD COLUMN project_origin TEXT NOT NULL DEFAULT 'unknown' CHECK (project_origin IN ('brownfield', 'greenfield', 'unknown'));
"#;

const MIGRATION_020_PROJECT_START_TARGETS_SQL: &str = r#"
    ALTER TABLE projects ADD COLUMN start_targets TEXT NOT NULL DEFAULT '[]';
"#;

const MIGRATION_024_AGENT_SESSION_REMOTE_VISIBILITY_SQL: &str = r#"
    ALTER TABLE agent_sessions ADD COLUMN remote_visible INTEGER NOT NULL DEFAULT 0 CHECK (remote_visible IN (0, 1));
"#;

const BUILTIN_AGENT_WORKFLOW_STRUCTURE_INSERT_SQL: &str = r#"
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
        ('generalist', 1, 'Agent', 'Agent', 'A do-anything agent with the full engineering toolset that recognises when a specialist agent would handle the task better and offers to route.', 'built_in', 'active', 'engineering', '2026-05-12T00:00:00Z');

    INSERT OR IGNORE INTO agent_definition_versions (
        definition_id,
        version,
        snapshot_json,
        validation_report_json,
        created_at
    )
    VALUES
        ('plan', 2, '{"schema":"xero.agent_definition.v1","id":"plan","version":2,"displayName":"Plan","shortLabel":"Plan","description":"Turn ambiguous work into an accepted, durable implementation plan without mutating repository files.","taskPurpose":"Interview the user, inspect project context when useful, draft a reproducible Plan Pack, and prepare Engineer handoff.","scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"planning","defaultApprovalMode":"suggest","allowedApprovalModes":["suggest"],"promptPolicy":"plan","toolPolicy":"planning","outputContract":"plan_pack","workflowContract":"Discover relevant repository context, draft the Plan Pack, then hold for user acceptance. Close the `plan_draft` todo to advance from drafting to acceptance.","finalResponseContract":"Produce the canonical Plan Pack summary and deterministic Engineer handoff prompt after acceptance.","workflowStructure":{"startPhaseId":"discover","phases":[{"id":"discover","title":"Discover","description":"Read at least one project file before drafting the plan.","allowedTools":["read","search","find","list","file_hash","git_status","git_diff","code_intel","lsp","workspace_index","tool_access","tool_search","project_context_search","project_context_get","todo"],"requiredChecks":[{"kind":"tool_succeeded","toolName":"read","minCount":1,"description":"Read at least one project file before drafting."}]},{"id":"draft","title":"Draft","description":"Draft the Plan Pack and close the `plan_draft` todo when ready for user review.","allowedTools":["read","search","find","list","file_hash","git_status","git_diff","code_intel","lsp","workspace_index","tool_access","tool_search","project_context_search","project_context_get","project_context_record","todo"],"requiredChecks":[{"kind":"todo_completed","todoId":"plan_draft","description":"Close the `plan_draft` todo when the Plan Pack is ready for review."}]},{"id":"accept","title":"Accept","description":"Hold for user acceptance and persist the accepted Plan Pack.","allowedTools":["read","search","find","list","file_hash","git_status","git_diff","code_intel","lsp","workspace_index","tool_access","tool_search","project_context_search","project_context_get","project_context_record","todo"]}]},"projectDataPolicy":{"required":true,"recordKinds":["agent_handoff","project_fact","decision","constraint","plan","question","context_note","diagnostic"],"structuredSchemas":["xero.project_record.v1","xero.plan_pack.v1"],"unstructuredScopes":["answer_note","session_summary","troubleshooting_note"],"memoryCandidateKinds":["project_fact","user_preference","decision","session_summary","troubleshooting"]},"attachedSkills":[]}', '{"status":"valid","source":"seed"}', '2026-05-11T00:00:00Z'),
        ('engineer', 2, '{"schema":"xero.agent_definition.v1","id":"engineer","version":2,"displayName":"Engineer","shortLabel":"Build","description":"Implement repository changes with the existing software-building toolset and safety gates.","taskPurpose":"Survey the change site, draft an implementation plan, apply scoped edits, then verify before declaring done.","scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"engineering","defaultApprovalMode":"suggest","allowedApprovalModes":["suggest","auto_edit","yolo"],"promptPolicy":"engineering","toolPolicy":"engineering","workflowContract":"Read before planning, plan before writing, write before verifying. Close the `implementation_plan` todo to leave the plan stage.","finalResponseContract":"Summarize changes, files touched, verification evidence, and follow-ups suitable for a same-type Engineer continuation.","workflowStructure":{"startPhaseId":"survey","phases":[{"id":"survey","title":"Survey","description":"Read the change site and at least one neighbor before planning.","allowedTools":["read","search","find","list","file_hash","git_status","git_diff","code_intel","lsp","workspace_index","tool_access","tool_search","project_context_search","project_context_get","environment_context","command_probe","todo"],"requiredChecks":[{"kind":"tool_succeeded","toolName":"read","minCount":2,"description":"Read the change site and one neighbor before planning."}]},{"id":"plan","title":"Plan","description":"Draft the implementation plan, then close the `implementation_plan` todo.","allowedTools":["read","search","find","list","file_hash","git_status","git_diff","code_intel","lsp","workspace_index","tool_access","tool_search","project_context_search","project_context_get","project_context_record","environment_context","command_probe","todo"],"requiredChecks":[{"kind":"todo_completed","todoId":"implementation_plan","description":"Close the `implementation_plan` todo with the agreed approach."}]},{"id":"implement","title":"Implement","description":"Apply edits with the planning tool surface plus write tools.","allowedTools":["read","search","find","list","file_hash","git_status","git_diff","code_intel","lsp","workspace_index","tool_access","tool_search","project_context_search","project_context_get","project_context_record","environment_context","command_probe","edit","write","notebook_edit","command_run","todo"],"requiredChecks":[{"kind":"tool_succeeded","toolName":"edit","minCount":1,"description":"Apply at least one edit before advancing to verify."}],"retryLimit":2},{"id":"verify","title":"Verify","description":"Run focused verification commands and report results.","allowedTools":["read","search","find","list","file_hash","git_status","git_diff","code_intel","lsp","workspace_index","tool_access","tool_search","project_context_search","project_context_get","project_context_record","environment_context","command_probe","edit","write","notebook_edit","command_run","command_verify","system_diagnostics","todo"]}]},"projectDataPolicy":{"required":true,"recordKinds":["project_fact","decision","constraint","plan","question","context_note","diagnostic","verification","agent_handoff"],"structuredSchemas":["xero.project_record.v1"],"unstructuredScopes":["answer_note","session_summary","troubleshooting_note"],"memoryCandidateKinds":["project_fact","user_preference","decision","session_summary","troubleshooting"]},"attachedSkills":[]}', '{"status":"valid","source":"seed"}', '2026-05-11T00:00:00Z'),
        ('debug', 2, '{"schema":"xero.agent_definition.v1","id":"debug","version":2,"displayName":"Debug","shortLabel":"Debug","description":"Investigate failures with structured evidence, hypotheses, fixes, verification, and durable debugging memory.","taskPurpose":"Reproduce the failure, form a falsifiable hypothesis, apply the narrowest fix, then verify the original failure and adjacent regressions.","scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"debugging","defaultApprovalMode":"suggest","allowedApprovalModes":["suggest","auto_edit","yolo"],"promptPolicy":"debugging","toolPolicy":"engineering","workflowContract":"Reproduce before hypothesizing, hypothesize before fixing, fix before verifying. Close `reproduction_steps` and `hypothesis` todos to advance.","finalResponseContract":"Provide symptom, root cause, fix, files changed, verification, and saved debugging knowledge.","workflowStructure":{"startPhaseId":"reproduce","phases":[{"id":"reproduce","title":"Reproduce","description":"Capture the failing symptom and a concrete way to trigger it. Close the `reproduction_steps` todo when done.","allowedTools":["read","search","find","list","file_hash","git_status","git_diff","command_probe","command_verify","system_diagnostics","environment_context","code_intel","lsp","workspace_index","tool_access","tool_search","project_context_search","project_context_get","todo"],"requiredChecks":[{"kind":"todo_completed","todoId":"reproduction_steps","description":"Close the `reproduction_steps` todo with concrete steps and observed symptom."}]},{"id":"hypothesize","title":"Hypothesize","description":"Form a falsifiable hypothesis. Close the `hypothesis` todo when ready.","allowedTools":["read","search","find","list","file_hash","git_status","git_diff","command_probe","command_verify","system_diagnostics","environment_context","code_intel","lsp","workspace_index","tool_access","tool_search","project_context_search","project_context_get","project_context_record","todo"],"requiredChecks":[{"kind":"todo_completed","todoId":"hypothesis","description":"Close the `hypothesis` todo with a falsifiable explanation."}]},{"id":"fix","title":"Fix","description":"Apply the narrowest fix that addresses the hypothesis.","allowedTools":["read","search","find","list","file_hash","git_status","git_diff","command_probe","command_verify","system_diagnostics","environment_context","code_intel","lsp","workspace_index","tool_access","tool_search","project_context_search","project_context_get","project_context_record","edit","write","command_run","todo"],"requiredChecks":[{"kind":"tool_succeeded","toolName":"edit","minCount":1,"description":"Apply at least one edit before advancing to verify."}],"retryLimit":3},{"id":"verify","title":"Verify","description":"Run the reproduction command and adjacent regression checks.","allowedTools":["read","search","find","list","file_hash","git_status","git_diff","command_probe","command_verify","system_diagnostics","environment_context","code_intel","lsp","workspace_index","tool_access","tool_search","project_context_search","project_context_get","project_context_record","edit","write","command_run","todo"]}]},"projectDataPolicy":{"required":true,"recordKinds":["project_fact","decision","constraint","finding","verification","artifact","context_note","diagnostic","question","agent_handoff"],"structuredSchemas":["xero.project_record.v1"],"unstructuredScopes":["answer_note","session_summary","troubleshooting_note"],"memoryCandidateKinds":["project_fact","decision","session_summary","troubleshooting"]},"attachedSkills":[]}', '{"status":"valid","source":"seed"}', '2026-05-11T00:00:00Z'),
        ('agent_create', 2, '{"schema":"xero.agent_definition.v1","id":"agent_create","version":2,"displayName":"Agent Create","shortLabel":"Create","description":"Interview the user and draft high-quality custom agent definitions.","taskPurpose":"Interview the user for agent purpose, scope, and risk tolerance, then draft a schema-validated custom agent definition.","scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"agent_builder","defaultApprovalMode":"suggest","allowedApprovalModes":["suggest"],"promptPolicy":"agent_create","toolPolicy":"agent_builder","workflowContract":"Complete the interview before drafting. Close the `interview_complete` todo to enter the drafting phase.","finalResponseContract":"Present a reviewable agent-definition draft with validation diagnostics.","workflowStructure":{"startPhaseId":"interview","phases":[{"id":"interview","title":"Interview","description":"Clarify purpose, scope, risk tolerance, and example tasks. Close the `interview_complete` todo when the interview is finished.","allowedTools":["read","search","find","tool_access","tool_search","todo"],"requiredChecks":[{"kind":"todo_completed","todoId":"interview_complete","description":"Close the `interview_complete` todo when the interview is finished."}]},{"id":"draft","title":"Draft","description":"Validate and save a draft definition through `agent_definition`.","allowedTools":["read","search","find","tool_access","tool_search","agent_definition","todo"]}]},"attachedSkills":[]}', '{"status":"valid","source":"seed"}', '2026-05-11T00:00:00Z'),
        ('generalist', 1, '{"schema":"xero.agent_definition.v1","id":"generalist","version":1,"displayName":"Agent","shortLabel":"Agent","description":"A do-anything agent with the full engineering toolset that recognises when a specialist agent would handle the task better and offers to route.","taskPurpose":"Handle any user request directly, or suggest routing to Plan, Engineer, or Debug when the request fits a specialist''s scope.","scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"engineering","defaultApprovalMode":"suggest","allowedApprovalModes":["suggest","auto_edit","yolo"],"promptPolicy":"generalist","toolPolicy":"engineering","outputContract":"answer","workflowContract":"Judge the task before starting work. If it fits a specialist (Plan, Engineer, Debug), call `suggest_routing`; otherwise proceed yourself with the engineering toolset.","finalResponseContract":"Answer directly for questions, summarize work for engineering tasks (files changed, verification, blockers).","workflowStructure":{"startPhaseId":"triage","phases":[{"id":"triage","title":"Triage","description":"Judge the task shape and either suggest routing or proceed.","allowedTools":["read","search","find","list","file_hash","git_status","git_diff","code_intel","lsp","workspace_index","tool_access","tool_search","project_context_search","project_context_get","environment_context","todo"]},{"id":"work","title":"Work","description":"Carry out the task using the full engineering toolset.","allowedTools":["read","search","find","list","file_hash","git_status","git_diff","code_intel","lsp","workspace_index","tool_access","tool_search","project_context_search","project_context_get","project_context_record","environment_context","command_probe","edit","write","notebook_edit","command_run","command_verify","system_diagnostics","todo"]}]},"projectDataPolicy":{"required":true,"recordKinds":["project_fact","decision","constraint","plan","question","context_note","diagnostic","verification","agent_handoff"],"structuredSchemas":["xero.project_record.v1"],"unstructuredScopes":["answer_note","session_summary","troubleshooting_note"],"memoryCandidateKinds":["project_fact","user_preference","decision","session_summary","troubleshooting"]},"attachedSkills":[]}', '{"status":"valid","source":"seed"}', '2026-05-12T00:00:00Z');

    UPDATE agent_definitions
    SET current_version = 2,
        updated_at = '2026-05-11T00:00:00Z'
    WHERE definition_id IN ('plan', 'engineer', 'debug', 'agent_create')
      AND current_version < 2;

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
        CHECK (base_capability_profile IN ('observe_only', 'computer_use', 'planning', 'repository_recon', 'engineering', 'debugging', 'agent_builder'))
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
        ('computer_use', 1, 'Computer Use', 'Computer', 'Follow direct user instructions by observing and controlling the local computer through bounded automation tools.', 'built_in', 'active', 'computer_use', '2026-05-24T00:00:00Z'),
        ('plan', 1, 'Plan', 'Plan', 'Turn ambiguous work into an accepted, durable implementation plan without mutating repository files.', 'built_in', 'active', 'planning', '2026-05-06T00:00:00Z'),
        ('engineer', 1, 'Engineer', 'Build', 'Implement repository changes with the existing software-building toolset and safety gates.', 'built_in', 'active', 'engineering', '2026-05-01T00:00:00Z'),
        ('debug', 1, 'Debug', 'Debug', 'Investigate failures with structured evidence, hypotheses, fixes, verification, and durable debugging memory.', 'built_in', 'active', 'debugging', '2026-05-01T00:00:00Z'),
        ('crawl', 1, 'Crawl', 'Crawl', 'Map an existing repository, identify stack, tests, commands, architecture, hot spots, and durable project facts without editing files.', 'built_in', 'active', 'repository_recon', '2026-05-06T00:00:00Z'),
        ('agent_create', 1, 'Agent Create', 'Create', 'Interview the user and draft high-quality custom agent definitions.', 'built_in', 'active', 'agent_builder', '2026-05-01T00:00:00Z');

    INSERT OR IGNORE INTO agent_definition_versions (
        definition_id,
        version,
        snapshot_json,
        validation_report_json,
        created_at
    )
    VALUES
        ('ask', 1, '{"id":"ask","version":1,"scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"observe_only","label":"Ask","shortLabel":"Ask","attachedSkills":[]}', '{"status":"valid","source":"seed"}', '2026-05-01T00:00:00Z'),
        ('computer_use', 1, '{"schema":"xero.agent_definition.v1","id":"computer_use","version":1,"displayName":"Computer Use","shortLabel":"Computer","description":"Follow direct user instructions by observing and controlling the local computer through bounded automation tools.","taskPurpose":"Observe visible computer state, perform bounded UI automation, ask before risky actions, and stop immediately when cancelled.","scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"computer_use","defaultApprovalMode":"suggest","allowedApprovalModes":["suggest"],"promptPolicy":"computer_use","toolPolicy":{"allowedTools":["tool_access","tool_search","todo","project_context_search","project_context_get","browser_observe","browser_control","browser","emulator","macos_automation","system_diagnostics_observe"],"deniedTools":["write","edit","patch","copy","fs_transaction","json_edit","toml_edit","yaml_edit","delete","rename","mkdir","command","command_run","command_session","command_session_start","command_session_read","command_session_stop","powershell","process_manager","git_status","git_diff","mcp","mcp_call_tool","skill","subagent"],"allowedEffectClasses":[],"browserControlAllowed":true,"commandAllowed":true,"destructiveWriteAllowed":false,"externalServiceAllowed":false,"skillRuntimeAllowed":false,"subagentAllowed":false},"outputContract":"answer","workflowContract":"","finalResponseContract":"Answer directly with what was done, what still needs user confirmation, or why the requested action was stopped. Do not include secrets.","projectDataPolicy":{"required":true,"recordKinds":["agent_handoff","project_fact","decision","constraint","question","context_note","diagnostic"],"structuredSchemas":["xero.project_record.v1"],"unstructuredScopes":["answer_note","session_summary","troubleshooting_note"],"memoryCandidateKinds":["project_fact","user_preference","decision","session_summary","troubleshooting"]},"attachedSkills":[]}', '{"status":"valid","source":"seed"}', '2026-05-24T00:00:00Z'),
        ('plan', 1, '{"schema":"xero.agent_definition.v1","id":"plan","version":1,"displayName":"Plan","shortLabel":"Plan","description":"Turn ambiguous work into an accepted, durable implementation plan without mutating repository files.","taskPurpose":"Interview the user, inspect project context when useful, draft a reproducible Plan Pack, and prepare Engineer handoff.","scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"planning","defaultApprovalMode":"suggest","allowedApprovalModes":["suggest"],"promptPolicy":"plan","toolPolicy":"planning","outputContract":"plan_pack","workflowContract":"Guide a user from vague task intent to an accepted xero.plan_pack.v1 without repository mutation.","finalResponseContract":"Produce the canonical Plan Pack summary and deterministic Engineer handoff prompt after acceptance.","projectDataPolicy":{"required":true,"recordKinds":["agent_handoff","project_fact","decision","constraint","plan","question","context_note","diagnostic"],"structuredSchemas":["xero.project_record.v1","xero.plan_pack.v1"],"unstructuredScopes":["answer_note","session_summary","troubleshooting_note"],"memoryCandidateKinds":["project_fact","user_preference","decision","session_summary","troubleshooting"]},"attachedSkills":[]}', '{"status":"valid","source":"seed"}', '2026-05-06T00:00:00Z'),
        ('engineer', 1, '{"id":"engineer","version":1,"scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"engineering","label":"Engineer","shortLabel":"Build","attachedSkills":[]}', '{"status":"valid","source":"seed"}', '2026-05-01T00:00:00Z'),
        ('debug', 1, '{"id":"debug","version":1,"scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"debugging","label":"Debug","shortLabel":"Debug","attachedSkills":[]}', '{"status":"valid","source":"seed"}', '2026-05-01T00:00:00Z'),
        ('crawl', 1, '{"schema":"xero.agent_definition.v1","id":"crawl","version":1,"displayName":"Crawl","shortLabel":"Crawl","description":"Map an existing repository, identify stack, tests, commands, architecture, hot spots, and durable project facts without editing files.","taskPurpose":"Read brownfield repository context and produce a structured crawl report for durable project memory.","scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"repository_recon","defaultApprovalMode":"suggest","allowedApprovalModes":["suggest"],"promptPolicy":"crawl","toolPolicy":"repository_recon","outputContract":"crawl_report","workflowContract":"Map the brownfield repository without mutating files or app state; use manifests, instructions, workspace index, safe git metadata, and read-only discovery.","finalResponseContract":"Produce a short summary plus a valid JSON crawl report payload using schema xero.project_crawl.report.v1.","projectDataPolicy":{"required":true,"recordKinds":["project_fact","constraint","finding","verification","artifact","context_note","diagnostic","question"],"structuredSchemas":["xero.project_record.v1","xero.project_crawl.report.v1","xero.project_crawl.project_overview.v1","xero.project_crawl.tech_stack.v1","xero.project_crawl.command_map.v1","xero.project_crawl.test_map.v1","xero.project_crawl.architecture_map.v1","xero.project_crawl.hotspots.v1","xero.project_crawl.constraints.v1","xero.project_crawl.unknowns.v1","xero.project_crawl.freshness.v1"],"unstructuredScopes":["answer_note","session_summary","artifact_excerpt","troubleshooting_note"],"memoryCandidateKinds":["project_fact","decision","session_summary","troubleshooting"]},"attachedSkills":[]}', '{"status":"valid","source":"seed"}', '2026-05-06T00:00:00Z'),
        ('agent_create', 1, '{"id":"agent_create","version":1,"scope":"built_in","lifecycleState":"active","baseCapabilityProfile":"agent_builder","label":"Agent Create","shortLabel":"Create","attachedSkills":[]}', '{"status":"valid","source":"seed"}', '2026-05-01T00:00:00Z');

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

    CREATE TABLE IF NOT EXISTS runtime_run_attached_skill_snapshots (
        project_id TEXT NOT NULL,
        run_id TEXT NOT NULL,
        snapshot_json TEXT NOT NULL,
        resolved_at TEXT NOT NULL,
        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        PRIMARY KEY (project_id, run_id),
        CHECK (project_id <> ''),
        CHECK (run_id <> ''),
        CHECK (snapshot_json <> '' AND json_valid(snapshot_json)),
        CHECK (resolved_at <> ''),
        CHECK (updated_at <> ''),
        FOREIGN KEY (project_id, run_id)
            REFERENCES agent_runs(project_id, run_id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_runtime_run_attached_skill_snapshots_updated
        ON runtime_run_attached_skill_snapshots(project_id, updated_at DESC);

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
            'subagent_lifecycle',
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

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

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
        let connection = migrate_to_latest_in_memory();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, PROJECT_DATABASE_SCHEMA_VERSION);
    }

    #[test]
    fn migrations_are_idempotent() {
        let mut connection = migrate_to_latest_in_memory();
        migrations()
            .to_latest(&mut connection)
            .expect("second migration is a no-op");
    }

    #[test]
    fn fresh_schema_contains_current_tables() {
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

        for table in [
            "projects",
            "repositories",
            "agent_sessions",
            "agent_runs",
            "agent_messages",
            "agent_definition_versions",
            "agent_definitions",
            "workflow_definitions",
            "workflow_definition_versions",
            "workflow_runs",
            "workflow_run_nodes",
            "workflow_run_edges",
            "workflow_artifacts",
            "workflow_gate_decisions",
            "workflow_loop_attempts",
            "workflow_events",
            "delivery_projects",
            "delivery_milestones",
            "delivery_requirements",
            "delivery_phases",
            "delivery_phase_context",
            "delivery_phase_plans",
            "delivery_phase_summaries",
            "delivery_verification_evidence",
            "delivery_deferred_items",
            "delivery_milestone_archives",
            "delivery_state_events",
            "runtime_run_attached_skill_snapshots",
            "workspace_index_metadata",
            "agent_embedding_backfill_jobs",
            "project_storage_maintenance_runs",
        ] {
            assert!(
                tables.contains(&table.to_string()),
                "fresh project databases should include `{table}`"
            );
        }
    }

    #[test]
    fn fresh_schema_has_current_project_and_session_columns() {
        let connection = migrate_to_latest_in_memory();

        let project_columns = table_columns(&connection, "projects");
        for column in ["project_origin", "start_targets"] {
            assert!(
                project_columns.contains(&column.to_string()),
                "projects should include `{column}`"
            );
        }

        let session_columns = table_columns(&connection, "agent_sessions");
        for column in ["remote_visible", "session_kind"] {
            assert!(
                session_columns.contains(&column.to_string()),
                "agent_sessions should include `{column}`"
            );
        }
    }

    #[test]
    fn fresh_schema_seeds_current_builtin_agent_versions() {
        let connection = migrate_to_latest_in_memory();
        let built_ins = collect_strings(
            &connection,
            r#"
            SELECT definition_id || ':' || current_version || ':' || display_name
            FROM agent_definitions
            WHERE scope = 'built_in'
            ORDER BY definition_id
            "#,
        );

        assert!(built_ins.contains(&"agent_create:3:Agent Create".to_string()));
        assert!(built_ins.contains(&"computer_use:1:Computer Use".to_string()));
        assert!(built_ins.contains(&"debug:2:Debug".to_string()));
        assert!(built_ins.contains(&"engineer:2:Engineer".to_string()));
        assert!(built_ins.contains(&"generalist:1:Agent".to_string()));
        assert!(built_ins.contains(&"plan:2:Plan".to_string()));

        let missing_attached_skills: i64 = connection
            .query_row(
                r#"
                SELECT COUNT(*)
                FROM agent_definition_versions
                WHERE json_extract(snapshot_json, '$.attachedSkills') IS NULL
                "#,
                [],
                |row| row.get(0),
            )
            .expect("count built-in snapshots missing attachedSkills");
        assert_eq!(missing_attached_skills, 0);
    }
}
