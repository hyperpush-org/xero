use super::support::*;

fn runtime_control_state(timestamp: &str) -> project_store::RuntimeRunControlStateRecord {
    project_store::build_runtime_run_control_state(
        "openai_codex",
        None,
        cadence_desktop_lib::commands::RuntimeRunApprovalModeDto::Suggest,
        timestamp,
        None,
    )
    .expect("build runtime control state")
}

pub(crate) fn resolve_operator_action_persists_decision_and_verification_rows() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-1";
    let repo_root = seed_project(&root, &app, project_id, "repo-1", "repo");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "review_worktree",
        "Repository has local changes",
        "Review the worktree before trusting subsequent agent actions.",
        "2026-04-13T20:00:49Z",
    )
    .expect("persist pending approval");
    assert_eq!(pending.status, OperatorApprovalStatus::Pending);

    let resolved = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Worktree reviewed and accepted.".into()),
        },
    )
    .expect("resolve operator action");

    assert_eq!(
        resolved.approval_request.status,
        OperatorApprovalStatus::Approved
    );
    assert_eq!(
        resolved.approval_request.decision_note.as_deref(),
        Some("Worktree reviewed and accepted.")
    );
    assert_eq!(
        resolved.verification_record.status,
        VerificationRecordStatus::Passed
    );
    assert_eq!(
        resolved.verification_record.source_action_id.as_deref(),
        Some(pending.action_id.as_str())
    );

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("load updated snapshot");
    assert_eq!(snapshot.approval_requests.len(), 1);
    assert_eq!(
        snapshot.approval_requests[0].status,
        OperatorApprovalStatus::Approved
    );
    assert_eq!(snapshot.verification_records.len(), 1);
    assert_eq!(
        snapshot.verification_records[0].status,
        VerificationRecordStatus::Passed
    );
    assert!(snapshot.resume_history.is_empty());
}

pub(crate) fn resume_operator_run_requires_approved_request_and_records_history() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-1";
    let repo_root = seed_project(&root, &app, project_id, "repo-1", "repo");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "review_worktree",
        "Repository has local changes",
        "Review the worktree before trusting subsequent agent actions.",
        "2026-04-13T20:00:49Z",
    )
    .expect("persist pending approval");

    let before_approval = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            user_answer: None,
        },
    )
    .expect_err("resume should require an approved request");
    assert_eq!(
        before_approval.code,
        "operator_resume_requires_approved_action"
    );

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Worktree reviewed and accepted.".into()),
        },
    )
    .expect("approve operator action");

    let resumed = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            user_answer: None,
        },
    )
    .expect("record resume history");

    assert_eq!(
        resumed.approval_request.status,
        OperatorApprovalStatus::Approved
    );
    assert_eq!(resumed.resume_entry.status, ResumeHistoryStatus::Started);
    assert_eq!(
        resumed.resume_entry.source_action_id.as_deref(),
        Some(pending.action_id.as_str())
    );

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("load updated snapshot");
    assert_eq!(snapshot.resume_history.len(), 1);
    assert_eq!(
        snapshot.resume_history[0].status,
        ResumeHistoryStatus::Started
    );
    assert_eq!(snapshot.verification_records.len(), 1);
}

pub(crate) fn resolve_operator_action_rejects_wrong_project_and_already_resolved_requests() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-1";
    let repo_root = seed_project(&root, &app, project_id, "repo-1", "repo");
    insert_other_project_rows(&repo_root);

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "review_worktree",
        "Repository has local changes",
        "Review the worktree before trusting subsequent agent actions.",
        "2026-04-13T20:00:49Z",
    )
    .expect("persist pending approval");

    let wrong_project = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: "other-action".into(),
            decision: "reject".into(),
            user_answer: Some("wrong project".into()),
        },
    )
    .expect_err("cross-project request should stay isolated");
    assert_eq!(wrong_project.code, "operator_action_not_found");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "reject".into(),
            user_answer: Some("Rejected after review.".into()),
        },
    )
    .expect("reject operator action");

    let duplicate = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id,
            decision: "approve".into(),
            user_answer: Some("should fail".into()),
        },
    )
    .expect_err("already-resolved request should be rejected");
    assert_eq!(duplicate.code, "operator_action_already_resolved");
}

pub(crate) fn runtime_scoped_resume_rejects_conflicting_user_answer_without_persisting_history() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-runtime-resume-conflict-1";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-runtime-resume-conflict-1",
        "repo-runtime-resume-conflict",
    );

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: "run-conflict-1".into(),
                runtime_kind: "openai_codex".into(),
                provider_id: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: project_store::RuntimeRunStatus::Running,
                transport: project_store::RuntimeRunTransportRecord {
                    kind: "tcp".into(),
                    endpoint: "127.0.0.1:9".into(),
                    liveness: project_store::RuntimeRunTransportLiveness::Reachable,
                },
                started_at: "2026-04-15T21:00:00Z".into(),
                last_heartbeat_at: Some("2026-04-15T21:00:05Z".into()),
                stopped_at: None,
                last_error: None,
                updated_at: "2026-04-15T21:00:05Z".into(),
            },
            checkpoint: None,
            control_state: Some(runtime_control_state("2026-04-15T21:00:05Z")),
        },
    )
    .expect("persist runtime run for conflicting-answer test");

    let persisted = project_store::upsert_runtime_action_required(
        &repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.into(),
            run_id: "run-conflict-1".into(),
            runtime_kind: "openai_codex".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            transport_endpoint: "127.0.0.1:9".into(),
            started_at: "2026-04-15T21:00:00Z".into(),
            last_heartbeat_at: Some("2026-04-15T21:00:05Z".into()),
            last_error: None,
            boundary_id: "boundary-1".into(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
            checkpoint_summary: "Detached runtime blocked on terminal input and is awaiting operator approval.".into(),
            created_at: "2026-04-15T21:00:06Z".into(),
        },
    )
    .expect("persist runtime action-required approval");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: persisted.approval_request.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("approved".into()),
        },
    )
    .expect("approve runtime action");

    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: persisted.approval_request.action_id,
            user_answer: Some("conflicting answer".into()),
        },
    )
    .expect_err("resume should reject conflicting runtime user answers");
    assert_eq!(error.code, "operator_resume_answer_conflict");

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("load project snapshot after conflicting resume attempt");
    assert!(snapshot.resume_history.is_empty());
    assert_eq!(snapshot.approval_requests.len(), 1);
    assert_eq!(
        snapshot.approval_requests[0].status,
        OperatorApprovalStatus::Approved
    );
}

pub(crate) fn runtime_scoped_resume_rejects_corrupted_approved_answer_metadata_without_persisting_history(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-runtime-resume-metadata-conflict-1";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-runtime-resume-metadata-conflict-1",
        "repo-runtime-resume-metadata-conflict",
    );

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: "run-metadata-conflict-1".into(),
                runtime_kind: "openai_codex".into(),
                provider_id: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: project_store::RuntimeRunStatus::Running,
                transport: project_store::RuntimeRunTransportRecord {
                    kind: "tcp".into(),
                    endpoint: "127.0.0.1:9".into(),
                    liveness: project_store::RuntimeRunTransportLiveness::Reachable,
                },
                started_at: "2026-04-15T21:06:00Z".into(),
                last_heartbeat_at: Some("2026-04-15T21:06:05Z".into()),
                stopped_at: None,
                last_error: None,
                updated_at: "2026-04-15T21:06:05Z".into(),
            },
            checkpoint: None,
            control_state: Some(runtime_control_state("2026-04-15T21:06:05Z")),
        },
    )
    .expect("persist runtime run for metadata-conflict test");

    let persisted = project_store::upsert_runtime_action_required(
        &repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.into(),
            run_id: "run-metadata-conflict-1".into(),
            runtime_kind: "openai_codex".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            transport_endpoint: "127.0.0.1:9".into(),
            started_at: "2026-04-15T21:06:00Z".into(),
            last_heartbeat_at: Some("2026-04-15T21:06:05Z".into()),
            last_error: None,
            boundary_id: "boundary-1".into(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
            checkpoint_summary:
                "Detached runtime blocked on terminal input and is awaiting operator approval."
                    .into(),
            created_at: "2026-04-15T21:06:06Z".into(),
        },
    )
    .expect("persist runtime action-required approval");

    let action_id = persisted.approval_request.action_id.clone();
    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("approved".into()),
        },
    )
    .expect("approve runtime action");

    let connection = open_state_connection(&repo_root);
    connection
        .execute(
            "UPDATE operator_approvals SET decision_note = 'tampered approved answer' WHERE project_id = ?1 AND action_id = ?2",
            params![project_id, action_id.as_str()],
        )
        .expect("corrupt approved decision metadata");

    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id,
            user_answer: None,
        },
    )
    .expect_err("resume should fail closed when approved answer metadata is inconsistent");
    assert_eq!(error.code, "operator_resume_answer_conflict");

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("load project snapshot after metadata-conflict resume failure");
    assert!(snapshot.resume_history.is_empty());
    assert_eq!(snapshot.approval_requests.len(), 1);
    assert_eq!(
        snapshot.approval_requests[0].status,
        OperatorApprovalStatus::Approved
    );
    assert_eq!(snapshot.verification_records.len(), 1);
}

pub(crate) fn runtime_scoped_approval_requires_non_secret_user_answer_at_resolve_time() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-runtime-resolve-answer-1";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-runtime-resolve-answer-1",
        "repo-runtime-resolve-answer",
    );

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: "run-require-answer-1".into(),
                runtime_kind: "openai_codex".into(),
                provider_id: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: project_store::RuntimeRunStatus::Running,
                transport: project_store::RuntimeRunTransportRecord {
                    kind: "tcp".into(),
                    endpoint: "127.0.0.1:9".into(),
                    liveness: project_store::RuntimeRunTransportLiveness::Reachable,
                },
                started_at: "2026-04-15T21:10:00Z".into(),
                last_heartbeat_at: Some("2026-04-15T21:10:05Z".into()),
                stopped_at: None,
                last_error: None,
                updated_at: "2026-04-15T21:10:05Z".into(),
            },
            checkpoint: None,
            control_state: Some(runtime_control_state("2026-04-15T21:10:05Z")),
        },
    )
    .expect("persist runtime run for resolve-answer test");

    let persisted = project_store::upsert_runtime_action_required(
        &repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.into(),
            run_id: "run-require-answer-1".into(),
            runtime_kind: "openai_codex".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            transport_endpoint: "127.0.0.1:9".into(),
            started_at: "2026-04-15T21:10:00Z".into(),
            last_heartbeat_at: Some("2026-04-15T21:10:05Z".into()),
            last_error: None,
            boundary_id: "boundary-1".into(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
            checkpoint_summary:
                "Detached runtime blocked on terminal input and is awaiting operator approval."
                    .into(),
            created_at: "2026-04-15T21:10:06Z".into(),
        },
    )
    .expect("persist runtime action-required approval");

    let action_id = persisted.approval_request.action_id.clone();

    let missing_answer = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: action_id.clone(),
            decision: "approve".into(),
            user_answer: None,
        },
    )
    .expect_err("runtime-scoped approvals should require a recorded answer");
    assert_eq!(missing_answer.code, "operator_action_answer_required");

    let snapshot_after_missing = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("load project snapshot after missing-answer failure");
    assert_eq!(snapshot_after_missing.approval_requests.len(), 1);
    assert_eq!(
        snapshot_after_missing.approval_requests[0].status,
        OperatorApprovalStatus::Pending
    );
    assert!(snapshot_after_missing.verification_records.is_empty());

    let secret_answer = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("oauth access_token=sk-live-secret".into()),
        },
    )
    .expect_err("secret-bearing answer payload should fail closed");
    assert_eq!(
        secret_answer.code,
        "operator_action_decision_payload_invalid"
    );

    let resolved = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("approved".into()),
        },
    )
    .expect("approve runtime-scoped action with a non-empty answer");
    assert_eq!(
        resolved.approval_request.status,
        OperatorApprovalStatus::Approved
    );
    assert_eq!(
        resolved.approval_request.decision_note.as_deref(),
        Some("approved")
    );
    assert_eq!(
        resolved.verification_record.status,
        VerificationRecordStatus::Passed
    );

    let prepared_resume = project_store::prepare_runtime_operator_run_resume(
        &repo_root, project_id, &action_id, None,
    )
    .expect("prepare runtime resume payload after successful approval")
    .expect("runtime-scoped approval should decode into resume payload");
    assert_eq!(prepared_resume.user_answer, "approved");

    let final_snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("load project snapshot after successful runtime approval");
    assert_eq!(final_snapshot.resume_history.len(), 0);
    assert_eq!(final_snapshot.verification_records.len(), 1);
}

pub(crate) fn runtime_scoped_approval_rejects_malformed_runtime_identity_without_partial_writes() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-runtime-resolve-malformed-1";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-runtime-resolve-malformed-1",
        "repo-runtime-resolve-malformed",
    );

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: "run-malformed-1".into(),
                runtime_kind: "openai_codex".into(),
                provider_id: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: project_store::RuntimeRunStatus::Running,
                transport: project_store::RuntimeRunTransportRecord {
                    kind: "tcp".into(),
                    endpoint: "127.0.0.1:9".into(),
                    liveness: project_store::RuntimeRunTransportLiveness::Reachable,
                },
                started_at: "2026-04-15T21:20:00Z".into(),
                last_heartbeat_at: Some("2026-04-15T21:20:05Z".into()),
                stopped_at: None,
                last_error: None,
                updated_at: "2026-04-15T21:20:05Z".into(),
            },
            checkpoint: None,
            control_state: Some(runtime_control_state("2026-04-15T21:20:05Z")),
        },
    )
    .expect("persist runtime run for malformed-identity test");

    let persisted = project_store::upsert_runtime_action_required(
        &repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.into(),
            run_id: "run-malformed-1".into(),
            runtime_kind: "openai_codex".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            transport_endpoint: "127.0.0.1:9".into(),
            started_at: "2026-04-15T21:20:00Z".into(),
            last_heartbeat_at: Some("2026-04-15T21:20:05Z".into()),
            last_error: None,
            boundary_id: "boundary-1".into(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
            checkpoint_summary:
                "Detached runtime blocked on terminal input and is awaiting operator approval."
                    .into(),
            created_at: "2026-04-15T21:20:06Z".into(),
        },
    )
    .expect("persist runtime action-required approval");

    let action_id = persisted.approval_request.action_id.clone();
    let connection = open_state_connection(&repo_root);
    connection
        .execute(
            "UPDATE operator_approvals SET action_type = 'terminal_input' WHERE project_id = ?1 AND action_id = ?2",
            params![project_id, action_id.as_str()],
        )
        .expect("corrupt runtime action identity");

    let malformed = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("approved".into()),
        },
    )
    .expect_err("malformed runtime action identity should fail closed at resolve time");
    assert_eq!(malformed.code, "operator_action_runtime_identity_invalid");

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("load project snapshot after malformed runtime resolve failure");
    assert_eq!(snapshot.approval_requests.len(), 1);
    assert_eq!(
        snapshot.approval_requests[0].status,
        OperatorApprovalStatus::Pending
    );
    assert!(snapshot.verification_records.is_empty());
    assert!(snapshot.resume_history.is_empty());
}

pub(crate) fn runtime_scoped_resume_rejects_already_resumed_autonomous_boundary_before_second_submit(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-runtime-autonomous-replay-1";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-runtime-autonomous-replay-1",
        "repo-runtime-autonomous-replay",
    );

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: "run-autonomous-replay-1".into(),
                runtime_kind: "openai_codex".into(),
                provider_id: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: project_store::RuntimeRunStatus::Running,
                transport: project_store::RuntimeRunTransportRecord {
                    kind: "tcp".into(),
                    endpoint: "127.0.0.1:9".into(),
                    liveness: project_store::RuntimeRunTransportLiveness::Reachable,
                },
                started_at: "2026-04-15T21:30:00Z".into(),
                last_heartbeat_at: Some("2026-04-15T21:30:05Z".into()),
                stopped_at: None,
                last_error: None,
                updated_at: "2026-04-15T21:30:05Z".into(),
            },
            checkpoint: None,
            control_state: Some(runtime_control_state("2026-04-15T21:30:05Z")),
        },
    )
    .expect("persist runtime run for autonomous replay test");

    let persisted = project_store::upsert_runtime_action_required(
        &repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.into(),
            run_id: "run-autonomous-replay-1".into(),
            runtime_kind: "openai_codex".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            transport_endpoint: "127.0.0.1:9".into(),
            started_at: "2026-04-15T21:30:00Z".into(),
            last_heartbeat_at: Some("2026-04-15T21:30:05Z".into()),
            last_error: None,
            boundary_id: "boundary-1".into(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
            checkpoint_summary:
                "Detached runtime blocked on terminal input and is awaiting operator approval."
                    .into(),
            created_at: "2026-04-15T21:30:06Z".into(),
        },
    )
    .expect("persist runtime action-required approval");

    let action_id = persisted.approval_request.action_id.clone();
    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("approved".into()),
        },
    )
    .expect("approve runtime action before replay test");

    let unit_id = "run-autonomous-replay-1:unit:1";
    let attempt_id = "run-autonomous-replay-1:unit:1:attempt:1";
    let boundary_id = "boundary-1";
    let timestamp = "2026-04-15T21:30:07Z";
    project_store::upsert_autonomous_run(
        &repo_root,
        &project_store::AutonomousRunUpsertRecord {
            run: project_store::AutonomousRunRecord {
                project_id: project_id.into(),
                run_id: "run-autonomous-replay-1".into(),
                runtime_kind: "openai_codex".into(),
                provider_id: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: project_store::AutonomousRunStatus::Running,
                active_unit_sequence: Some(1),
                duplicate_start_detected: false,
                duplicate_start_run_id: None,
                duplicate_start_reason: None,
                started_at: timestamp.into(),
                last_heartbeat_at: Some(timestamp.into()),
                last_checkpoint_at: Some(timestamp.into()),
                paused_at: None,
                cancelled_at: None,
                completed_at: None,
                crashed_at: None,
                stopped_at: None,
                pause_reason: None,
                cancel_reason: None,
                crash_reason: None,
                last_error: None,
                updated_at: timestamp.into(),
            },
            unit: Some(project_store::AutonomousUnitRecord {
                project_id: project_id.into(),
                run_id: "run-autonomous-replay-1".into(),
                unit_id: unit_id.into(),
                sequence: 1,
                kind: project_store::AutonomousUnitKind::Researcher,
                status: project_store::AutonomousUnitStatus::Active,
                summary: "Autonomous attempt resumed after operator approval.".into(),
                boundary_id: None,
                workflow_linkage: None,
                started_at: timestamp.into(),
                finished_at: None,
                updated_at: timestamp.into(),
                last_error: None,
            }),
            attempt: Some(project_store::AutonomousUnitAttemptRecord {
                project_id: project_id.into(),
                run_id: "run-autonomous-replay-1".into(),
                unit_id: unit_id.into(),
                attempt_id: attempt_id.into(),
                attempt_number: 1,
                child_session_id: "child-session-1".into(),
                status: project_store::AutonomousUnitStatus::Active,
                boundary_id: None,
                workflow_linkage: None,
                started_at: timestamp.into(),
                finished_at: None,
                updated_at: timestamp.into(),
                last_error: None,
            }),
            artifacts: vec![
                project_store::AutonomousUnitArtifactRecord {
                    project_id: project_id.into(),
                    run_id: "run-autonomous-replay-1".into(),
                    unit_id: unit_id.into(),
                    attempt_id: attempt_id.into(),
                    artifact_id: format!("{attempt_id}:boundary:{boundary_id}:blocked"),
                    artifact_kind: "verification_evidence".into(),
                    status: project_store::AutonomousUnitArtifactStatus::Recorded,
                    summary: "Autonomous attempt blocked on `Terminal input required` and is waiting for operator action.".into(),
                    content_hash: None,
                    payload: Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(
                        project_store::AutonomousVerificationEvidencePayloadRecord {
                            project_id: project_id.into(),
                            run_id: "run-autonomous-replay-1".into(),
                            unit_id: unit_id.into(),
                            attempt_id: attempt_id.into(),
                            artifact_id: format!("{attempt_id}:boundary:{boundary_id}:blocked"),
                            evidence_kind: "terminal_input_required".into(),
                            label: "Terminal input required".into(),
                            outcome: project_store::AutonomousVerificationOutcomeRecord::Blocked,
                            command_result: None,
                            action_id: Some(action_id.clone()),
                            boundary_id: Some(boundary_id.into()),
                        },
                    )),
                    created_at: timestamp.into(),
                    updated_at: timestamp.into(),
                },
                project_store::AutonomousUnitArtifactRecord {
                    project_id: project_id.into(),
                    run_id: "run-autonomous-replay-1".into(),
                    unit_id: unit_id.into(),
                    attempt_id: attempt_id.into(),
                    artifact_id: format!("{attempt_id}:boundary:{boundary_id}:resumed"),
                    artifact_kind: "verification_evidence".into(),
                    status: project_store::AutonomousUnitArtifactStatus::Recorded,
                    summary: "Autonomous attempt resumed boundary `Terminal input required` after operator approval.".into(),
                    content_hash: None,
                    payload: Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(
                        project_store::AutonomousVerificationEvidencePayloadRecord {
                            project_id: project_id.into(),
                            run_id: "run-autonomous-replay-1".into(),
                            unit_id: unit_id.into(),
                            attempt_id: attempt_id.into(),
                            artifact_id: format!("{attempt_id}:boundary:{boundary_id}:resumed"),
                            evidence_kind: "operator_resume".into(),
                            label: "Terminal input required".into(),
                            outcome: project_store::AutonomousVerificationOutcomeRecord::Passed,
                            command_result: None,
                            action_id: Some(action_id.clone()),
                            boundary_id: Some(boundary_id.into()),
                        },
                    )),
                    created_at: timestamp.into(),
                    updated_at: timestamp.into(),
                },
            ],
        },
    )
    .expect("persist autonomous blocked/resumed evidence for replay guard test");

    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: action_id.clone(),
            user_answer: None,
        },
    )
    .expect_err("already-resumed autonomous boundary should be rejected before a second submit");
    assert_eq!(error.code, "autonomous_resume_already_completed");

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("load project snapshot after autonomous replay guard failure");
    assert_eq!(snapshot.resume_history.len(), 1);
    assert_eq!(
        snapshot.resume_history[0].status,
        ResumeHistoryStatus::Failed
    );
    assert_eq!(
        snapshot.resume_history[0].source_action_id.as_deref(),
        Some(action_id.as_str())
    );

    let runtime_run = project_store::load_runtime_run(&repo_root, project_id)
        .expect("load runtime run after autonomous replay guard failure")
        .expect("runtime run should still exist after autonomous replay guard failure");
    assert_eq!(runtime_run.run.run_id, "run-autonomous-replay-1");
}
