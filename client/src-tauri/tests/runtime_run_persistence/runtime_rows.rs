use super::support::*;

pub(crate) fn runtime_run_recovery_distinguishes_running_stale_stopped_and_failed_states() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");

    assert!(project_store::load_runtime_run(
        &repo_root,
        project_id,
        project_store::DEFAULT_AGENT_SESSION_ID
    )
    .expect("load empty runtime run state")
    .is_none());

    let run_id = "run-1";
    let running = sample_run(project_id, run_id);
    let first = project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: running.clone(),
            checkpoint: None,
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist running runtime run without checkpoints");
    assert_eq!(first.run.status, project_store::RuntimeRunStatus::Running);
    assert!(first.checkpoints.is_empty());
    assert_eq!(first.last_checkpoint_sequence, 0);
    assert!(first.last_checkpoint_at.is_none());
    assert_eq!(first.controls.active.model_id, "openai_codex");
    assert_eq!(
        first.controls.active.thinking_effort,
        Some(xero_desktop_lib::commands::ProviderModelThinkingEffortDto::Medium)
    );
    assert_eq!(
        first.controls.active.approval_mode,
        xero_desktop_lib::commands::RuntimeRunApprovalModeDto::Suggest
    );
    assert!(first.controls.pending.is_none());

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
            control_state: None,
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
            control_state: None,
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
    assert_eq!(
        recovered.controls.active.model_id,
        first.controls.active.model_id
    );
    assert_eq!(recovered.controls, first.controls);

    let stale = project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                last_heartbeat_at: Some("2020-04-15T19:00:35Z".into()),
                updated_at: "2020-04-15T19:00:35Z".into(),
                ..running.clone()
            },
            checkpoint: None,
            control_state: None,
        },
    )
    .expect("persist stale runtime run");
    assert_eq!(stale.run.status, project_store::RuntimeRunStatus::Stale);
    assert_eq!(stale.controls, first.controls);

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
            control_state: None,
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
            control_state: None,
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
    assert_eq!(failed.controls, first.controls);
}

pub(crate) fn runtime_run_persists_active_and_pending_control_snapshots_with_queued_prompt() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-controls";
    let control_state = project_store::build_runtime_run_control_state_with_plan_mode(
        xero_desktop_lib::commands::RuntimeAgentIdDto::Engineer,
        "openai_codex",
        Some(xero_desktop_lib::commands::ProviderModelThinkingEffortDto::High),
        xero_desktop_lib::commands::RuntimeRunApprovalModeDto::AutoEdit,
        false,
        "2099-04-15T19:00:00Z",
        Some("Summarize the active worktree and propose the next action."),
    )
    .expect("build queued control state");

    let persisted = project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: None,
            control_state: Some(control_state.clone()),
        },
    )
    .expect("persist runtime run with queued control snapshot");

    assert_eq!(persisted.controls, control_state);
    let pending = persisted
        .controls
        .pending
        .as_ref()
        .expect("queued prompt should persist pending control snapshot");
    assert_eq!(pending.model_id, "openai_codex");
    assert_eq!(
        pending.thinking_effort,
        Some(xero_desktop_lib::commands::ProviderModelThinkingEffortDto::High)
    );
    assert_eq!(
        pending.approval_mode,
        xero_desktop_lib::commands::RuntimeRunApprovalModeDto::AutoEdit
    );
    assert_eq!(pending.revision, 2);
    assert_eq!(
        pending.queued_prompt.as_deref(),
        Some("Summarize the active worktree and propose the next action.")
    );
    assert_eq!(
        pending.queued_prompt_at.as_deref(),
        Some("2099-04-15T19:00:00Z")
    );

    let recovered = project_store::load_runtime_run(
        &repo_root,
        project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
    )
    .expect("reload runtime run with queued control snapshot")
    .expect("runtime run should exist");
    assert_eq!(recovered.controls, control_state);
}

pub(crate) fn runtime_run_persistence_isolates_runs_by_agent_session() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-sessions";
    let repo_root = seed_project(&root, project_id, "repo-sessions", "repo-sessions");
    let second_session = project_store::create_agent_session(
        &repo_root,
        &project_store::AgentSessionCreateRecord {
            project_id: project_id.into(),
            title: "Parallel".into(),
            summary: "Independent owned-agent run".into(),
            selected: false,
        },
    )
    .expect("create secondary agent session");

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, "run-main"),
            checkpoint: Some(sample_checkpoint(
                project_id,
                "run-main",
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Main session supervisor launched.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist default session runtime run");

    let mut secondary_run = sample_run(project_id, "run-parallel");
    secondary_run.agent_session_id = second_session.agent_session_id.clone();
    secondary_run.transport.endpoint = "127.0.0.1:5566".into();
    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: secondary_run,
            checkpoint: Some(sample_checkpoint(
                project_id,
                "run-parallel",
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Parallel session supervisor launched.",
                "2099-04-15T19:01:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:01:00Z")),
        },
    )
    .expect("persist secondary session runtime run");

    let default_snapshot = project_store::load_runtime_run(
        &repo_root,
        project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
    )
    .expect("load default session runtime run")
    .expect("default session runtime run should exist");
    let secondary_snapshot =
        project_store::load_runtime_run(&repo_root, project_id, &second_session.agent_session_id)
            .expect("load secondary session runtime run")
            .expect("secondary session runtime run should exist");

    assert_eq!(default_snapshot.run.run_id, "run-main");
    assert_eq!(
        default_snapshot.run.agent_session_id,
        project_store::DEFAULT_AGENT_SESSION_ID
    );
    assert_eq!(
        default_snapshot.checkpoints[0].summary,
        "Main session supervisor launched."
    );
    assert_eq!(secondary_snapshot.run.run_id, "run-parallel");
    assert_eq!(
        secondary_snapshot.run.agent_session_id,
        second_session.agent_session_id
    );
    assert_eq!(
        secondary_snapshot.checkpoints[0].summary,
        "Parallel session supervisor launched."
    );

    let sessions = project_store::list_agent_sessions(&repo_root, project_id, false)
        .expect("list agent sessions");
    let default_session = sessions
        .iter()
        .find(|session| session.agent_session_id == project_store::DEFAULT_AGENT_SESSION_ID)
        .expect("default session should exist");
    let stored_second_session = sessions
        .iter()
        .find(|session| session.agent_session_id == secondary_snapshot.run.agent_session_id)
        .expect("secondary session should exist");
    assert_eq!(default_session.last_run_id.as_deref(), Some("run-main"));
    assert_eq!(
        stored_second_session.last_run_id.as_deref(),
        Some("run-parallel")
    );
}

pub(crate) fn runtime_run_checkpoint_writes_reject_secret_bearing_summaries_and_preserve_prior_sequence(
) {
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
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
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
            control_state: None,
        },
    )
    .expect_err("secret-bearing checkpoint summary should fail closed");
    assert_eq!(error.code, "runtime_run_checkpoint_invalid");

    let recovered = project_store::load_runtime_run(
        &repo_root,
        project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
    )
    .expect("reload runtime run after rejected checkpoint")
    .expect("runtime run should still exist");
    assert_eq!(recovered.last_checkpoint_sequence, 1);
    assert_eq!(recovered.checkpoints.len(), 1);

    let database_bytes = std::fs::read(database_path_for_repo(&repo_root)).expect("read db bytes");
    let database_text = String::from_utf8_lossy(&database_bytes);
    assert!(!database_text.contains("sk-live-secret"));
    assert!(!database_text.contains("redirect_uri=http://127.0.0.1:1455/auth/callback"));
}

pub(crate) fn runtime_run_decode_fails_closed_for_malformed_status_transport_checkpoint_kind_and_controls(
) {
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
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
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
    let error = project_store::load_runtime_run(
        &repo_root,
        project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
    )
    .expect_err("malformed runtime run status should fail closed");
    assert_eq!(error.code, "runtime_run_decode_failed");

    connection
        .execute(
            "UPDATE runtime_runs SET status = 'running', transport_endpoint = '' WHERE project_id = ?1",
            [project_id],
        )
        .expect("corrupt transport metadata");
    let error = project_store::load_runtime_run(
        &repo_root,
        project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
    )
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
    let error = project_store::load_runtime_run(
        &repo_root,
        project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
    )
    .expect_err("malformed checkpoint kind should fail closed");
    assert_eq!(error.code, "runtime_run_checkpoint_decode_failed");

    connection
        .execute(
            "UPDATE runtime_run_checkpoints SET kind = 'bootstrap' WHERE project_id = ?1 AND run_id = ?2 AND sequence = 1",
            params![project_id, run_id],
        )
        .expect("repair checkpoint kind");
    connection
        .execute(
            "UPDATE runtime_runs SET control_state_json = '{\"active\":{\"modelId\":\" \" ,\"approvalMode\":\"suggest\",\"revision\":1,\"appliedAt\":\"2099-04-15T19:00:00Z\"}}' WHERE project_id = ?1",
            [project_id],
        )
        .expect("corrupt control model id");
    let error = project_store::load_runtime_run(
        &repo_root,
        project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
    )
    .expect_err("blank active control model id should fail closed");
    assert_eq!(error.code, "runtime_run_decode_failed");

    connection
        .execute(
            "UPDATE runtime_runs SET control_state_json = '{\"active\":{\"modelId\":\"openai_codex\",\"thinkingEffort\":\"ludicrous\",\"approvalMode\":\"suggest\",\"revision\":1,\"appliedAt\":\"2099-04-15T19:00:00Z\"}}' WHERE project_id = ?1",
            [project_id],
        )
        .expect("corrupt thinking effort enum");
    let error = project_store::load_runtime_run(
        &repo_root,
        project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
    )
    .expect_err("bogus thinking effort should fail closed");
    assert_eq!(error.code, "runtime_run_decode_failed");

    connection
        .execute(
            "UPDATE runtime_runs SET control_state_json = '{\"active\":{\"modelId\":\"openai_codex\",\"approvalMode\":\"\",\"revision\":1,\"appliedAt\":\"2099-04-15T19:00:00Z\"}}' WHERE project_id = ?1",
            [project_id],
        )
        .expect("corrupt approval mode enum");
    let error = project_store::load_runtime_run(
        &repo_root,
        project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
    )
    .expect_err("blank approval mode should fail closed");
    assert_eq!(error.code, "runtime_run_decode_failed");

    connection
        .execute(
            "UPDATE runtime_runs SET control_state_json = '{\"pending\":{\"modelId\":\"openai_codex\",\"approvalMode\":\"suggest\",\"revision\":2,\"queuedAt\":\"2099-04-15T19:00:00Z\"}}' WHERE project_id = ?1",
            [project_id],
        )
        .expect("remove active control snapshot");
    let error = project_store::load_runtime_run(
        &repo_root,
        project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
    )
    .expect_err("missing active control snapshot should fail closed");
    assert_eq!(error.code, "runtime_run_decode_failed");
}

pub(crate) fn runtime_run_checkpoint_sequence_must_increase_monotonically() {
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
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
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
            control_state: None,
        },
    )
    .expect_err("duplicate checkpoint sequence should fail closed");
    assert_eq!(error.code, "runtime_run_checkpoint_sequence_invalid");

    let recovered = project_store::load_runtime_run(
        &repo_root,
        project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
    )
    .expect("reload runtime run after rejected sequence")
    .expect("runtime run should still exist");
    assert_eq!(recovered.last_checkpoint_sequence, 1);
    assert_eq!(recovered.checkpoints.len(), 1);
    assert_eq!(recovered.checkpoints[0].summary, "First checkpoint.");
    assert_eq!(recovered.controls.active.model_id, "openai_codex");
}

pub(crate) fn runtime_run_rotation_clears_prior_autonomous_projection() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let first_run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, first_run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                first_run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "First run launched.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist first runtime run");

    let connection = open_state_connection(&repo_root);
    connection
        .execute(
            r#"
            INSERT INTO autonomous_runs (
                project_id,
                agent_session_id,
                run_id,
                runtime_kind,
                provider_id,
                supervisor_kind,
                status,
                duplicate_start_detected,
                started_at,
                last_heartbeat_at,
                stopped_at,
                last_error_code,
                last_error_message,
                updated_at
            )
            VALUES (?1, ?2, ?3, 'openai_codex', 'openai_codex', 'owned_agent', 'failed', 0, ?4, ?5, ?6, 'openai_codex_auth_failed', 'Provider returned HTTP 401.', ?6)
            "#,
            params![
                project_id,
                project_store::DEFAULT_AGENT_SESSION_ID,
                first_run_id,
                "2099-04-15T19:00:00Z",
                "2099-04-15T19:00:20Z",
                "2099-04-15T19:00:30Z",
            ],
        )
        .expect("seed prior autonomous projection");
    drop(connection);

    let second_run_id = "run-2";
    let rotated = project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, second_run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                second_run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Replacement run launched.",
                "2099-04-15T19:01:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:01:00Z")),
        },
    )
    .expect("rotate runtime run after prior autonomous projection");

    assert_eq!(rotated.run.run_id, second_run_id);

    let connection = open_state_connection(&repo_root);
    let autonomous_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM autonomous_runs WHERE project_id = ?1",
            [project_id],
            |row| row.get(0),
        )
        .expect("count autonomous projections");
    assert_eq!(autonomous_count, 0);
}
