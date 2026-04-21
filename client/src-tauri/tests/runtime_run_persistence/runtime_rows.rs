use super::support::*;
pub(crate) fn legacy_repo_local_state_is_upgraded_before_runtime_run_reads() {
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

pub(crate) fn runtime_run_recovery_distinguishes_running_stale_stopped_and_failed_states() {
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

pub(crate) fn runtime_run_checkpoint_writes_reject_secret_bearing_summaries_and_preserve_prior_sequence() {
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

pub(crate) fn runtime_run_decode_fails_closed_for_malformed_status_transport_and_checkpoint_kind() {
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
