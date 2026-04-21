use super::support::*;
pub(crate) fn autonomous_run_persistence_tracks_current_unit_duplicate_start_and_cancel_metadata() {
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
    assert_eq!(
        persisted.run.status,
        project_store::AutonomousRunStatus::Running
    );
    assert_eq!(persisted.run.active_unit_sequence, Some(1));
    assert_eq!(persisted.unit.as_ref().map(|unit| unit.sequence), Some(1));
    assert_eq!(
        persisted.unit.as_ref().map(|unit| unit.kind.clone()),
        Some(project_store::AutonomousUnitKind::Researcher)
    );
    assert_eq!(
        persisted
            .attempt
            .as_ref()
            .map(|attempt| attempt.child_session_id.as_str()),
        Some("child-session-1")
    );
    assert_eq!(persisted.history.len(), 1);

    let cancelled = project_store::upsert_autonomous_run(
        &repo_root,
        &project_store::AutonomousRunUpsertRecord {
            run: project_store::AutonomousRunRecord {
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
                ..sample_autonomous_run_record(project_id, run_id)
            },
            unit: Some(project_store::AutonomousUnitRecord {
                status: project_store::AutonomousUnitStatus::Cancelled,
                finished_at: Some("2099-04-15T19:01:05Z".into()),
                updated_at: "2099-04-15T19:01:05Z".into(),
                ..sample_autonomous_unit(project_id, run_id)
            }),
            attempt: Some(project_store::AutonomousUnitAttemptRecord {
                status: project_store::AutonomousUnitStatus::Cancelled,
                finished_at: Some("2099-04-15T19:01:05Z".into()),
                updated_at: "2099-04-15T19:01:05Z".into(),
                ..sample_autonomous_attempt(project_id, run_id)
            }),
            artifacts: Vec::new(),
        },
    )
    .expect("persist cancelled autonomous run");
    assert_eq!(
        cancelled.run.status,
        project_store::AutonomousRunStatus::Cancelled
    );
    assert!(cancelled.run.duplicate_start_detected);
    assert_eq!(
        cancelled.run.cancelled_at.as_deref(),
        Some("2099-04-15T19:01:05Z")
    );
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
    assert_eq!(recovered.unit.as_ref().map(|unit| unit.sequence), Some(1));
    assert_eq!(
        recovered
            .attempt
            .as_ref()
            .map(|attempt| attempt.attempt_number),
        Some(1)
    );
}

pub(crate) fn autonomous_run_persistence_persists_explicit_workflow_linkage_and_replays_idempotently() {
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
    .expect("persist runtime run for workflow linkage projection");
    seed_autonomous_workflow_linkage_rows(&repo_root, project_id);

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.unit.as_mut().expect("unit").workflow_linkage =
        Some(sample_autonomous_workflow_linkage());
    payload.attempt.as_mut().expect("attempt").workflow_linkage =
        Some(sample_autonomous_workflow_linkage());

    let first = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect("persist autonomous run with workflow linkage");
    assert_eq!(
        first
            .unit
            .as_ref()
            .and_then(|unit| unit.workflow_linkage.as_ref())
            .cloned(),
        Some(sample_autonomous_workflow_linkage())
    );
    assert_eq!(
        first
            .attempt
            .as_ref()
            .and_then(|attempt| attempt.workflow_linkage.as_ref())
            .cloned(),
        Some(sample_autonomous_workflow_linkage())
    );

    let second = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect("replay autonomous run with workflow linkage");
    assert_eq!(second.unit, first.unit);
    assert_eq!(second.attempt, first.attempt);

    let stored_linkage: (String, String, Option<String>, String, String) =
        open_state_connection(&repo_root)
            .query_row(
                r#"
            SELECT
                workflow_node_id,
                workflow_transition_id,
                workflow_causal_transition_id,
                workflow_handoff_transition_id,
                workflow_handoff_package_hash
            FROM autonomous_units
            WHERE project_id = ?1 AND run_id = ?2
            "#,
                params![project_id, run_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .expect("read stored autonomous unit linkage");
    assert_eq!(
        stored_linkage,
        (
            "workflow-research".to_string(),
            "auto:txn-002:workflow-discussion:workflow-research".to_string(),
            Some("txn-001".to_string()),
            "auto:txn-002:workflow-discussion:workflow-research".to_string(),
            "f2a21cec422a39086c026fa96b38f2875b83faabc49461e979c5504c34b2640e".to_string(),
        )
    );

    let recovered = project_store::load_autonomous_run(&repo_root, project_id)
        .expect("reload autonomous run with workflow linkage")
        .expect("autonomous run with workflow linkage should exist");
    assert_eq!(
        recovered
            .unit
            .as_ref()
            .and_then(|unit| unit.workflow_linkage.as_ref())
            .cloned(),
        Some(sample_autonomous_workflow_linkage())
    );
    assert_eq!(
        recovered
            .attempt
            .as_ref()
            .and_then(|attempt| attempt.workflow_linkage.as_ref())
            .cloned(),
        Some(sample_autonomous_workflow_linkage())
    );
}

pub(crate) fn autonomous_run_persistence_rejects_blank_workflow_linkage_fields() {
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
    .expect("persist runtime run before invalid linkage request");

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.unit.as_mut().expect("unit").workflow_linkage =
        Some(project_store::AutonomousWorkflowLinkageRecord {
            workflow_node_id: "".into(),
            ..sample_autonomous_workflow_linkage()
        });
    payload.attempt.as_mut().expect("attempt").workflow_linkage =
        Some(sample_autonomous_workflow_linkage());

    let error = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect_err("blank workflow linkage fields should be rejected");
    assert_eq!(error.code, "autonomous_run_request_invalid");
}

pub(crate) fn autonomous_run_decode_fails_closed_for_cross_project_workflow_linkage_tampering() {
    let root = tempfile::tempdir().expect("temp dir");
    let repo_root_one = seed_project(&root, "project-1", "repo-1", "repo-one");
    let repo_root_two = seed_project(&root, "project-2", "repo-2", "repo-two");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root_one,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run("project-1", run_id),
            checkpoint: Some(sample_checkpoint(
                "project-1",
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Supervisor launched and connected to the project PTY.",
                "2099-04-15T19:00:20Z",
            )),
        },
    )
    .expect("persist runtime run for first project");
    project_store::upsert_runtime_run(
        &repo_root_two,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run("project-2", run_id),
            checkpoint: Some(sample_checkpoint(
                "project-2",
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Supervisor launched and connected to the project PTY.",
                "2099-04-15T19:00:20Z",
            )),
        },
    )
    .expect("persist runtime run for second project");
    seed_autonomous_workflow_linkage_rows(&repo_root_one, "project-1");
    seed_autonomous_workflow_linkage_rows(&repo_root_two, "project-2");

    let mut payload = sample_autonomous_run("project-1", run_id);
    payload.unit.as_mut().expect("unit").workflow_linkage =
        Some(sample_autonomous_workflow_linkage());
    payload.attempt.as_mut().expect("attempt").workflow_linkage =
        Some(sample_autonomous_workflow_linkage());
    project_store::upsert_autonomous_run(&repo_root_one, &payload)
        .expect("persist autonomous run before cross-project tampering");

    open_state_connection(&repo_root_one)
        .execute(
            r#"
            UPDATE autonomous_units
            SET workflow_transition_id = 'project-2-transition',
                workflow_handoff_transition_id = 'project-2-transition',
                workflow_handoff_package_hash = 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'
            WHERE project_id = ?1 AND run_id = ?2
            "#,
            params!["project-1", run_id],
        )
        .expect("tamper autonomous unit workflow linkage");

    let error = project_store::load_autonomous_run(&repo_root_one, "project-1")
        .expect_err("cross-project workflow linkage should fail closed");
    assert_eq!(error.code, "runtime_run_decode_failed");
}

pub(crate) fn autonomous_run_decode_fails_closed_when_unit_row_is_missing() {
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
    project_store::upsert_autonomous_run(&repo_root, &sample_autonomous_run(project_id, run_id))
        .expect("persist autonomous run before corruption");

    let connection = open_state_connection(&repo_root);
    connection
        .execute(
            "DELETE FROM autonomous_units WHERE project_id = ?1 AND run_id = ?2",
            params![project_id, run_id],
        )
        .expect("delete active autonomous unit row");

    let error = project_store::load_autonomous_run(&repo_root, project_id)
        .expect_err("missing active autonomous unit row should fail closed");
    assert_eq!(error.code, "runtime_run_decode_failed");
}
