use cadence_desktop_lib::{
    commands::{
        ApplyWorkflowTransitionRequestDto, ApplyWorkflowTransitionResponseDto,
        AutonomousArtifactPayloadDto, AutonomousCommandResultDto, AutonomousLifecycleReasonDto,
        AutonomousRunDto, AutonomousRunRecoveryStateDto, AutonomousRunStateDto,
        AutonomousRunStatusDto, AutonomousToolCallStateDto, AutonomousToolResultPayloadDto,
        AutonomousUnitArtifactDto, AutonomousUnitArtifactStatusDto, AutonomousUnitAttemptDto,
        AutonomousUnitDto, AutonomousUnitHistoryEntryDto, AutonomousUnitKindDto,
        AutonomousUnitStatusDto, AutonomousWorkflowLinkageDto, BranchSummaryDto,
        CancelAutonomousRunRequestDto, ChangeKind, CommandError, CommandErrorClass,
        GetAutonomousRunRequestDto, GetRuntimeRunRequestDto, ImportRepositoryRequestDto,
        ListNotificationDispatchesRequestDto, ListNotificationDispatchesResponseDto,
        ListNotificationRoutesRequestDto, ListNotificationRoutesResponseDto,
        ListProjectsResponseDto, NotificationDispatchDto, NotificationDispatchOutcomeStatusDto,
        NotificationDispatchStatusDto, NotificationReplyClaimDto, NotificationReplyClaimStatusDto,
        NotificationRouteDto, NotificationRouteKindDto, OperatorApprovalDto,
        OperatorApprovalStatus, PlanningLifecycleProjectionDto, PlanningLifecycleStageDto,
        PlanningLifecycleStageKindDto, ProjectIdRequestDto, ProjectSnapshotResponseDto,
        ProjectSummaryDto, ProjectUpdateReason, ProjectUpdatedPayloadDto,
        RecordNotificationDispatchOutcomeRequestDto, RecordNotificationDispatchOutcomeResponseDto,
        RepositoryDiffRequestDto, RepositoryDiffResponseDto, RepositoryDiffScope,
        RepositoryStatusChangedPayloadDto, RepositoryStatusEntryDto, RepositoryStatusResponseDto,
        RepositorySummaryDto, ResolveOperatorActionRequestDto, ResolveOperatorActionResponseDto,
        ResumeHistoryEntryDto, ResumeHistoryStatus, ResumeOperatorRunRequestDto,
        ResumeOperatorRunResponseDto, RuntimeAuthPhase, RuntimeAuthStatusDto, RuntimeDiagnosticDto,
        RuntimeRunCheckpointDto, RuntimeRunCheckpointKindDto, RuntimeRunDiagnosticDto,
        RuntimeRunDto, RuntimeRunStatusDto, RuntimeRunTransportDto, RuntimeRunTransportLivenessDto,
        RuntimeRunUpdatedPayloadDto, RuntimeStreamItemDto, RuntimeStreamItemKind,
        RuntimeUpdatedPayloadDto, StartAutonomousRunRequestDto, StartOpenAiLoginRequestDto,
        StartRuntimeRunRequestDto, StopRuntimeRunRequestDto, SubmitNotificationReplyRequestDto,
        SubmitNotificationReplyResponseDto, SubmitOpenAiCallbackRequestDto,
        SubscribeRuntimeStreamRequestDto, SubscribeRuntimeStreamResponseDto,
        SyncNotificationAdaptersRequestDto, SyncNotificationAdaptersResponseDto,
        UpsertNotificationRouteCredentialsRequestDto,
        UpsertNotificationRouteCredentialsResponseDto, UpsertNotificationRouteRequestDto,
        UpsertNotificationRouteResponseDto, UpsertWorkflowGraphRequestDto,
        UpsertWorkflowGraphResponseDto, VerificationRecordDto, VerificationRecordStatus,
        WorkflowAutomaticDispatchOutcomeDto, WorkflowAutomaticDispatchPackageOutcomeDto,
        WorkflowAutomaticDispatchPackageStatusDto, WorkflowAutomaticDispatchStatusDto,
        WorkflowGateMetadataDto, WorkflowGateStateDto, WorkflowGraphEdgeDto,
        WorkflowGraphGateRequestDto, WorkflowGraphNodeDto, WorkflowHandoffPackageDto,
        WorkflowTransitionEventDto, WorkflowTransitionGateDecisionDto,
        WorkflowTransitionGateUpdateRequestDto, APPLY_WORKFLOW_TRANSITION_COMMAND,
        CANCEL_AUTONOMOUS_RUN_COMMAND, CANCEL_OPENAI_CODEX_AUTH_COMMAND,
        COMPLETE_OPENAI_CODEX_AUTH_COMMAND, GET_AUTONOMOUS_RUN_COMMAND,
        GET_PROJECT_SNAPSHOT_COMMAND, GET_REPOSITORY_DIFF_COMMAND, GET_REPOSITORY_STATUS_COMMAND,
        GET_RUNTIME_AUTH_STATUS_COMMAND, GET_RUNTIME_RUN_COMMAND, IMPORT_REPOSITORY_COMMAND,
        LIST_NOTIFICATION_DISPATCHES_COMMAND, LIST_NOTIFICATION_ROUTES_COMMAND,
        LIST_PROJECTS_COMMAND, PROJECT_UPDATED_EVENT, RECORD_NOTIFICATION_DISPATCH_OUTCOME_COMMAND,
        REFRESH_OPENAI_CODEX_AUTH_COMMAND, REGISTERED_COMMAND_NAMES, REMOVE_PROJECT_COMMAND,
        REPOSITORY_STATUS_CHANGED_EVENT, RESOLVE_OPERATOR_ACTION_COMMAND,
        RESUME_OPERATOR_RUN_COMMAND, RUNTIME_RUN_UPDATED_EVENT, RUNTIME_UPDATED_EVENT,
        START_AUTONOMOUS_RUN_COMMAND, START_OPENAI_CODEX_AUTH_COMMAND, START_RUNTIME_RUN_COMMAND,
        STOP_RUNTIME_RUN_COMMAND, SUBMIT_NOTIFICATION_REPLY_COMMAND,
        SUBSCRIBE_RUNTIME_STREAM_COMMAND, SYNC_NOTIFICATION_ADAPTERS_COMMAND,
        UPSERT_NOTIFICATION_ROUTE_COMMAND, UPSERT_NOTIFICATION_ROUTE_CREDENTIALS_COMMAND,
        UPSERT_WORKFLOW_GRAPH_COMMAND,
    },
    configure_builder_with_state,
    state::DesktopState,
};
use serde_json::{json, Value};
use tempfile::TempDir;

fn build_mock_app() -> (tauri::App<tauri::test::MockRuntime>, TempDir) {
    let root = tempfile::tempdir().expect("temp dir");
    let registry_path = root.path().join("app-data").join("project-registry.json");
    let auth_store_path = root.path().join("app-data").join("openai-auth.json");
    let credential_store_path = root
        .path()
        .join("app-data")
        .join("notification-credentials.json");
    let runtime_settings_path = root.path().join("app-data").join("runtime-settings.json");
    let openrouter_credential_path = root
        .path()
        .join("app-data")
        .join("openrouter-credentials.json");
    let state = DesktopState::default()
        .with_registry_file_override(registry_path)
        .with_auth_store_file_override(auth_store_path)
        .with_notification_credential_store_file_override(credential_store_path)
        .with_runtime_settings_file_override(runtime_settings_path)
        .with_openrouter_credential_file_override(openrouter_credential_path);

    let app = configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app");

    (app, root)
}

fn invoke_request(command: &str, payload: Value) -> tauri::webview::InvokeRequest {
    tauri::webview::InvokeRequest {
        cmd: command.into(),
        callback: tauri::ipc::CallbackFn(0),
        error: tauri::ipc::CallbackFn(1),
        url: "http://tauri.localhost".parse().expect("valid mock URL"),
        body: tauri::ipc::InvokeBody::Json(payload),
        headers: Default::default(),
        invoke_key: tauri::test::INVOKE_KEY.to_string(),
    }
}

fn channel_string() -> String {
    serde_json::to_value(tauri::ipc::Channel::<RuntimeStreamItemDto>::new(|_| Ok(())))
        .expect("channel should serialize")
        .as_str()
        .expect("channel should serialize to string")
        .to_string()
}

fn sample_project() -> ProjectSummaryDto {
    ProjectSummaryDto {
        id: "project-1".into(),
        name: "cadence".into(),
        description: "Desktop shell".into(),
        milestone: "M001".into(),
        total_phases: 5,
        completed_phases: 1,
        active_phase: 2,
        branch: Some("main".into()),
        runtime: None,
    }
}

fn sample_repository() -> RepositorySummaryDto {
    RepositorySummaryDto {
        id: "repo-1".into(),
        project_id: "project-1".into(),
        root_path: "/tmp/cadence".into(),
        display_name: "cadence".into(),
        branch: Some("main".into()),
        head_sha: Some("abc123".into()),
        is_git_repo: true,
    }
}

fn sample_transition_event() -> WorkflowTransitionEventDto {
    WorkflowTransitionEventDto {
        id: 12,
        transition_id: "txn-002".into(),
        causal_transition_id: Some("txn-001".into()),
        from_node_id: "plan".into(),
        to_node_id: "execute".into(),
        transition_kind: "advance".into(),
        gate_decision: WorkflowTransitionGateDecisionDto::Approved,
        gate_decision_context: Some("operator-approved".into()),
        created_at: "2026-04-15T18:01:00Z".into(),
    }
}

fn sample_handoff_package() -> WorkflowHandoffPackageDto {
    WorkflowHandoffPackageDto {
        id: 21,
        project_id: "project-1".into(),
        handoff_transition_id: "auto:txn-002:workflow-discussion:workflow-research".into(),
        causal_transition_id: Some("txn-001".into()),
        from_node_id: "workflow-discussion".into(),
        to_node_id: "workflow-research".into(),
        transition_kind: "advance".into(),
        package_payload: "{\"schemaVersion\":1,\"triggerTransition\":{\"transitionId\":\"auto:txn-002:workflow-discussion:workflow-research\"}}".into(),
        package_hash: "a18fc57e3d2b8f4ef67f5b50f37ba7d85f49a1be987f17fa9dc0ad5a64ff8322".into(),
        created_at: "2026-04-15T18:01:01Z".into(),
    }
}

fn sample_automatic_dispatch_outcome() -> WorkflowAutomaticDispatchOutcomeDto {
    WorkflowAutomaticDispatchOutcomeDto {
        status: WorkflowAutomaticDispatchStatusDto::Applied,
        transition_event: Some(sample_transition_event()),
        handoff_package: Some(WorkflowAutomaticDispatchPackageOutcomeDto {
            status: WorkflowAutomaticDispatchPackageStatusDto::Persisted,
            package: Some(sample_handoff_package()),
            code: None,
            message: None,
        }),
        code: None,
        message: None,
    }
}

fn sample_skipped_automatic_dispatch_outcome() -> WorkflowAutomaticDispatchOutcomeDto {
    WorkflowAutomaticDispatchOutcomeDto {
        status: WorkflowAutomaticDispatchStatusDto::Skipped,
        transition_event: None,
        handoff_package: None,
        code: Some("workflow_transition_gate_unmet".into()),
        message: Some(
            "Cadence skipped automatic dispatch from `execute` because continuation edges are still blocked by unresolved gates: execute->verify:advance gate=verify_gate [verify:verify_gate:pending]. Persisted pending operator approval `flow:workflow-auto-dispatch:project-1:transition-gate-pause-1:gate:verify:verify_gate:approve_verify` for deterministic replay.".into(),
        ),
    }
}

fn sample_autonomous_run(duplicate_start_detected: bool) -> AutonomousRunDto {
    AutonomousRunDto {
        project_id: "project-1".into(),
        run_id: "run-1".into(),
        runtime_kind: "openai_codex".into(),
        provider_id: "openai_codex".into(),
        supervisor_kind: "detached_pty".into(),
        status: AutonomousRunStatusDto::Stale,
        recovery_state: AutonomousRunRecoveryStateDto::RecoveryRequired,
        active_unit_id: Some("run-1:unit:researcher".into()),
        active_attempt_id: Some("run-1:unit:researcher:attempt:1".into()),
        duplicate_start_detected,
        duplicate_start_run_id: duplicate_start_detected.then_some("run-1".into()),
        duplicate_start_reason: duplicate_start_detected.then_some(
            "Cadence reused the already-active autonomous run for this project instead of launching a duplicate supervisor."
                .into(),
        ),
        started_at: "2026-04-15T23:10:00Z".into(),
        last_heartbeat_at: Some("2026-04-15T23:10:01Z".into()),
        last_checkpoint_at: Some("2026-04-15T23:10:02Z".into()),
        paused_at: None,
        cancelled_at: None,
        completed_at: None,
        crashed_at: Some("2026-04-15T23:10:03Z".into()),
        stopped_at: None,
        pause_reason: None,
        cancel_reason: None,
        crash_reason: Some(AutonomousLifecycleReasonDto {
            code: "runtime_supervisor_connect_failed".into(),
            message: "Cadence could not connect to the detached supervisor control endpoint."
                .into(),
        }),
        last_error_code: Some("runtime_supervisor_connect_failed".into()),
        last_error: Some(RuntimeRunDiagnosticDto {
            code: "runtime_supervisor_connect_failed".into(),
            message: "Cadence could not connect to the detached supervisor control endpoint."
                .into(),
        }),
        updated_at: "2026-04-15T23:10:03Z".into(),
    }
}

fn sample_autonomous_workflow_linkage() -> AutonomousWorkflowLinkageDto {
    AutonomousWorkflowLinkageDto {
        workflow_node_id: "workflow-research".into(),
        transition_id: "auto:txn-002:workflow-discussion:workflow-research".into(),
        causal_transition_id: Some("txn-001".into()),
        handoff_transition_id: "auto:txn-002:workflow-discussion:workflow-research".into(),
        handoff_package_hash: "a18fc57e3d2b8f4ef67f5b50f37ba7d85f49a1be987f17fa9dc0ad5a64ff8322"
            .into(),
    }
}

fn sample_autonomous_unit() -> AutonomousUnitDto {
    AutonomousUnitDto {
        project_id: "project-1".into(),
        run_id: "run-1".into(),
        unit_id: "run-1:unit:researcher".into(),
        sequence: 1,
        kind: AutonomousUnitKindDto::Researcher,
        status: AutonomousUnitStatusDto::Active,
        summary: "Researcher child session launched.".into(),
        boundary_id: None,
        workflow_linkage: Some(sample_autonomous_workflow_linkage()),
        started_at: "2026-04-15T23:10:00Z".into(),
        finished_at: None,
        updated_at: "2026-04-15T23:10:03Z".into(),
        last_error_code: Some("runtime_supervisor_connect_failed".into()),
        last_error: Some(RuntimeRunDiagnosticDto {
            code: "runtime_supervisor_connect_failed".into(),
            message: "Cadence could not connect to the detached supervisor control endpoint."
                .into(),
        }),
    }
}

fn sample_autonomous_attempt() -> AutonomousUnitAttemptDto {
    AutonomousUnitAttemptDto {
        project_id: "project-1".into(),
        run_id: "run-1".into(),
        unit_id: "run-1:unit:researcher".into(),
        attempt_id: "run-1:unit:researcher:attempt:1".into(),
        attempt_number: 1,
        child_session_id: "child-session-1".into(),
        status: AutonomousUnitStatusDto::Active,
        boundary_id: None,
        workflow_linkage: Some(sample_autonomous_workflow_linkage()),
        started_at: "2026-04-15T23:10:00Z".into(),
        finished_at: None,
        updated_at: "2026-04-15T23:10:03Z".into(),
        last_error_code: Some("runtime_supervisor_connect_failed".into()),
        last_error: Some(RuntimeRunDiagnosticDto {
            code: "runtime_supervisor_connect_failed".into(),
            message: "Cadence could not connect to the detached supervisor control endpoint."
                .into(),
        }),
    }
}

fn sample_autonomous_artifact() -> AutonomousUnitArtifactDto {
    AutonomousUnitArtifactDto {
        project_id: "project-1".into(),
        run_id: "run-1".into(),
        unit_id: "run-1:unit:researcher".into(),
        attempt_id: "run-1:unit:researcher:attempt:1".into(),
        artifact_id: "artifact-tool-result".into(),
        artifact_kind: "tool_result".into(),
        status: AutonomousUnitArtifactStatusDto::Recorded,
        summary: "Shell tool result persisted for downstream verification.".into(),
        content_hash: Some(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
        ),
        payload: Some(AutonomousArtifactPayloadDto::ToolResult(
            AutonomousToolResultPayloadDto {
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                unit_id: "run-1:unit:researcher".into(),
                attempt_id: "run-1:unit:researcher:attempt:1".into(),
                artifact_id: "artifact-tool-result".into(),
                tool_call_id: "tool-call-1".into(),
                tool_name: "shell.exec".into(),
                tool_state: AutonomousToolCallStateDto::Succeeded,
                command_result: Some(AutonomousCommandResultDto {
                    exit_code: Some(0),
                    timed_out: false,
                    summary: "Command exited successfully after persisting structured evidence."
                        .into(),
                }),
                action_id: Some("action-1".into()),
                boundary_id: Some("boundary-1".into()),
            },
        )),
        created_at: "2026-04-15T23:10:03Z".into(),
        updated_at: "2026-04-15T23:10:03Z".into(),
    }
}

fn sample_autonomous_history() -> Vec<AutonomousUnitHistoryEntryDto> {
    vec![AutonomousUnitHistoryEntryDto {
        unit: sample_autonomous_unit(),
        latest_attempt: Some(sample_autonomous_attempt()),
        artifacts: vec![sample_autonomous_artifact()],
    }]
}

fn sample_snapshot() -> ProjectSnapshotResponseDto {
    ProjectSnapshotResponseDto {
        project: sample_project(),
        repository: Some(sample_repository()),
        phases: vec![cadence_desktop_lib::commands::PhaseSummaryDto {
            id: 2,
            name: "Live state".into(),
            description: "Connect the desktop shell".into(),
            status: cadence_desktop_lib::commands::PhaseStatus::Active,
            current_step: Some(cadence_desktop_lib::commands::PhaseStep::Execute),
            task_count: 2,
            completed_tasks: 1,
            summary: None,
        }],
        lifecycle: PlanningLifecycleProjectionDto {
            stages: vec![PlanningLifecycleStageDto {
                stage: PlanningLifecycleStageKindDto::Discussion,
                node_id: "discussion".into(),
                status: cadence_desktop_lib::commands::PhaseStatus::Active,
                action_required: true,
                last_transition_at: Some("2026-04-13T20:00:49Z".into()),
            }],
        },
        approval_requests: vec![OperatorApprovalDto {
            action_id: "review_worktree".into(),
            session_id: Some("session-1".into()),
            flow_id: Some("flow-1".into()),
            action_type: "review_worktree".into(),
            title: "Repository has local changes".into(),
            detail: "cadence currently has 3 changed file(s). Review the worktree before trusting subsequent agent actions.".into(),
            gate_node_id: None,
            gate_key: None,
            transition_from_node_id: None,
            transition_to_node_id: None,
            transition_kind: None,
            user_answer: None,
            status: OperatorApprovalStatus::Pending,
            decision_note: None,
            created_at: "2026-04-13T20:00:49Z".into(),
            updated_at: "2026-04-13T20:00:49Z".into(),
            resolved_at: None,
        }],
        verification_records: vec![VerificationRecordDto {
            id: 7,
            source_action_id: Some("review_worktree".into()),
            status: VerificationRecordStatus::Passed,
            summary: "Reviewed repository status before resume.".into(),
            detail: Some("Worktree inspection completed without blocking changes.".into()),
            recorded_at: "2026-04-13T20:05:12Z".into(),
        }],
        resume_history: vec![ResumeHistoryEntryDto {
            id: 4,
            source_action_id: Some("review_worktree".into()),
            session_id: Some("session-1".into()),
            status: ResumeHistoryStatus::Started,
            summary: "Operator resumed the selected project's runtime session.".into(),
            created_at: "2026-04-13T20:06:33Z".into(),
        }],
        handoff_packages: vec![sample_handoff_package()],
        autonomous_run: Some(sample_autonomous_run(false)),
        autonomous_unit: Some(sample_autonomous_unit()),
    }
}

#[test]
fn builder_boots_and_registered_commands_return_expected_contract_shapes() {
    let (app, _temp_root) = build_mock_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("failed to create mock webview window");

    assert_eq!(
        REGISTERED_COMMAND_NAMES.len(),
        31,
        "expected thirty-one desktop commands"
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(LIST_PROJECTS_COMMAND, json!({})),
        Ok(ListProjectsResponseDto {
            projects: Vec::new(),
        }),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            REMOVE_PROJECT_COMMAND,
            json!({ "request": { "projectId": "project-1" } }),
        ),
        Ok(ListProjectsResponseDto {
            projects: Vec::new(),
        }),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            IMPORT_REPOSITORY_COMMAND,
            json!({ "request": { "path": "" } }),
        ),
        Err(CommandError::invalid_request("path")),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            GET_PROJECT_SNAPSHOT_COMMAND,
            json!({ "request": { "projectId": "project-1" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            GET_REPOSITORY_STATUS_COMMAND,
            json!({ "request": { "projectId": "project-1" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            GET_REPOSITORY_DIFF_COMMAND,
            json!({ "request": { "projectId": "project-1", "scope": "unstaged" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            GET_AUTONOMOUS_RUN_COMMAND,
            json!({ "request": { "projectId": "project-1" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            GET_RUNTIME_RUN_COMMAND,
            json!({ "request": { "projectId": "project-1" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            GET_RUNTIME_AUTH_STATUS_COMMAND,
            json!({ "request": { "projectId": "project-1" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            cadence_desktop_lib::commands::GET_RUNTIME_SETTINGS_COMMAND,
            json!({}),
        ),
        Ok(cadence_desktop_lib::commands::RuntimeSettingsDto {
            provider_id: "openai_codex".into(),
            model_id: "openai_codex".into(),
            openrouter_api_key_configured: false,
        }),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            cadence_desktop_lib::commands::UPSERT_RUNTIME_SETTINGS_COMMAND,
            json!({
                "request": {
                    "providerId": "openrouter",
                    "modelId": "openai/gpt-4o-mini",
                    "openrouterApiKey": "credential-value-1"
                }
            }),
        ),
        Ok(cadence_desktop_lib::commands::RuntimeSettingsDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key_configured: true,
        }),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            START_OPENAI_CODEX_AUTH_COMMAND,
            json!({ "request": { "projectId": "project-1", "originator": "tests" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            COMPLETE_OPENAI_CODEX_AUTH_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1",
                    "flowId": "flow-1",
                    "manualInput": "http://localhost:1455/auth/callback?code=abc"
                }
            }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            REFRESH_OPENAI_CODEX_AUTH_COMMAND,
            json!({ "request": { "projectId": "project-1" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            START_AUTONOMOUS_RUN_COMMAND,
            json!({ "request": { "projectId": "project-1" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            START_RUNTIME_RUN_COMMAND,
            json!({ "request": { "projectId": "project-1" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            CANCEL_AUTONOMOUS_RUN_COMMAND,
            json!({ "request": { "projectId": "project-1", "runId": "run-1" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            STOP_RUNTIME_RUN_COMMAND,
            json!({ "request": { "projectId": "project-1", "runId": "run-1" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            SUBSCRIBE_RUNTIME_STREAM_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1",
                    "channel": channel_string(),
                    "itemKinds": ["transcript", "tool", "action_required"]
                }
            }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            RESOLVE_OPERATOR_ACTION_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1",
                    "actionId": "session:session-1:review_worktree",
                    "decision": "approve",
                    "userAnswer": "Looks good"
                }
            }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            RESUME_OPERATOR_RUN_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1",
                    "actionId": "session:session-1:review_worktree",
                    "userAnswer": "Looks good"
                }
            }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            LIST_NOTIFICATION_ROUTES_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1"
                }
            }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            UPSERT_NOTIFICATION_ROUTE_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1",
                    "routeId": "route-discord",
                    "routeKind": "discord",
                    "routeTarget": "123456789012345678",
                    "enabled": true,
                    "metadataJson": "{\"channel\":\"ops\"}",
                    "updatedAt": "2026-04-16T20:06:33Z"
                }
            }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            UPSERT_NOTIFICATION_ROUTE_CREDENTIALS_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1",
                    "routeId": "route-discord",
                    "routeKind": "discord",
                    "credentials": {
                        "botToken": "discord-bot-token",
                        "webhookUrl": "https://discord.com/api/webhooks/1/2"
                    },
                    "updatedAt": "2026-04-16T20:06:33Z"
                }
            }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            UPSERT_NOTIFICATION_ROUTE_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1",
                    "routeId": "route-discord",
                    "routeKind": "email",
                    "routeTarget": "123456789012345678",
                    "enabled": true,
                    "metadataJson": "{\"channel\":\"ops\"}",
                    "updatedAt": "2026-04-16T20:06:33Z"
                }
            }),
        ),
        Err(CommandError::user_fixable(
            "notification_route_request_invalid",
            "Cadence does not support notification route kind `email`. Allowed kinds: telegram, discord.",
        )),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            UPSERT_NOTIFICATION_ROUTE_CREDENTIALS_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1",
                    "routeId": "route-discord",
                    "routeKind": "email",
                    "credentials": {
                        "webhookUrl": "https://discord.com/api/webhooks/1/2"
                    },
                    "updatedAt": "2026-04-16T20:06:33Z"
                }
            }),
        ),
        Err(CommandError::user_fixable(
            "notification_route_credentials_request_invalid",
            "Cadence does not support notification route kind `email`. Allowed kinds: telegram, discord.",
        )),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            LIST_NOTIFICATION_DISPATCHES_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1"
                }
            }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            RECORD_NOTIFICATION_DISPATCH_OUTCOME_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1",
                    "actionId": "session:session-1:review_worktree",
                    "routeId": "route-discord",
                    "status": "sent",
                    "attemptedAt": "2026-04-16T20:06:33Z",
                    "errorCode": null,
                    "errorMessage": null
                }
            }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            SUBMIT_NOTIFICATION_REPLY_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1",
                    "actionId": "session:session-1:review_worktree",
                    "routeId": "route-discord",
                    "correlationKey": "nfy:11111111111111111111111111111111",
                    "responderId": "operator-a",
                    "replyText": "Looks good",
                    "decision": "approve",
                    "receivedAt": "2026-04-16T20:06:34Z"
                }
            }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            SYNC_NOTIFICATION_ADAPTERS_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1"
                }
            }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            UPSERT_WORKFLOW_GRAPH_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1",
                    "nodes": [
                        {
                            "nodeId": "plan",
                            "phaseId": 1,
                            "sortOrder": 1,
                            "name": "Plan",
                            "description": "Plan workflow",
                            "status": "active",
                            "currentStep": "plan",
                            "taskCount": 1,
                            "completedTasks": 0,
                            "summary": null
                        }
                    ],
                    "edges": [],
                    "gates": []
                }
            }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            APPLY_WORKFLOW_TRANSITION_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1",
                    "transitionId": "txn-1",
                    "causalTransitionId": null,
                    "fromNodeId": "plan",
                    "toNodeId": "execute",
                    "transitionKind": "advance",
                    "gateDecision": "approved",
                    "gateDecisionContext": null,
                    "gateUpdates": [],
                    "occurredAt": "2026-04-13T20:06:33Z"
                }
            }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            UPSERT_WORKFLOW_GRAPH_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1",
                    "nodes": [
                        {
                            "nodeId": "plan",
                            "phaseId": 1,
                            "sortOrder": 1,
                            "name": "Plan",
                            "description": "Plan workflow",
                            "status": "active",
                            "currentStep": "plan",
                            "taskCount": 1,
                            "completedTasks": 0,
                            "summary": null
                        }
                    ],
                    "edges": [],
                    "gates": [
                        {
                            "nodeId": "plan",
                            "gateKey": "execution_gate",
                            "gateState": "mystery",
                            "actionType": null,
                            "title": null,
                            "detail": null,
                            "decisionContext": null
                        }
                    ]
                }
            }),
        ),
        Err(CommandError::user_fixable(
            "workflow_graph_request_invalid",
            "Cadence does not support workflow gate_state `mystery`. Allowed states: pending, satisfied, blocked, skipped.",
        )),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            APPLY_WORKFLOW_TRANSITION_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1",
                    "transitionId": "txn-1",
                    "causalTransitionId": null,
                    "fromNodeId": "plan",
                    "toNodeId": "execute",
                    "transitionKind": "advance",
                    "gateDecision": "allow",
                    "gateDecisionContext": null,
                    "gateUpdates": [],
                    "occurredAt": "2026-04-13T20:06:33Z"
                }
            }),
        ),
        Err(CommandError::user_fixable(
            "workflow_transition_request_invalid",
            "Cadence does not support gateDecision `allow`. Allowed values: approved, rejected, blocked, not_applicable.",
        )),
    );
}

#[test]
fn config_and_capability_files_lock_the_packaged_vite_shell_and_auth_opener_permissions() {
    let tauri_config: Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).expect("valid tauri config json");
    let capability: Value = serde_json::from_str(include_str!("../capabilities/default.json"))
        .expect("valid capability json");

    assert_eq!(
        tauri_config["build"]["devUrl"],
        json!("http://localhost:3000"),
        "tauri.conf build.devUrl drifted; desktop shell must keep the local Vite dev endpoint"
    );
    assert_eq!(
        tauri_config["build"]["frontendDist"],
        json!("../dist"),
        "tauri.conf build.frontendDist drifted; packaged app must load the built web assets"
    );
    assert_eq!(
        tauri_config["app"]["windows"][0]["label"],
        json!("main"),
        "tauri.conf app.windows[0].label drifted; capability scoping depends on the main window label"
    );
    assert_eq!(
        tauri_config["app"]["security"]["capabilities"],
        json!(["default"]),
        "tauri.conf app.security.capabilities drifted; only the default least-privilege capability is allowed"
    );

    let bundle_active = tauri_config["bundle"]["active"]
        .as_bool()
        .expect("tauri.conf bundle.active must be a boolean");
    assert!(
        bundle_active,
        "tauri.conf bundle.active must stay true so packaged-app posture remains explicit"
    );

    assert_eq!(
        capability["identifier"],
        json!("default"),
        "capabilities/default.json identifier drifted; bootstrap must lock to default"
    );
    assert_eq!(
        capability["windows"],
        json!(["main"]),
        "capabilities/default.json windows drifted; permission scope must stay bound to the main window"
    );

    let permissions = capability["permissions"]
        .as_array()
        .expect("capabilities/default.json permissions must be an array");
    assert_eq!(
        permissions.len(),
        7,
        "capabilities/default.json permissions must stay scoped to core runtime, required window controls, dialog access, and the auth opener allowlist"
    );
    assert_eq!(
        permissions[0],
        json!("core:default"),
        "capabilities/default.json permissions[0] drifted; core:default is required"
    );
    assert_eq!(
        permissions[1],
        json!("core:window:allow-start-dragging"),
        "capabilities/default.json permissions[1] drifted; frameless Cadence shell must retain start-dragging"
    );
    assert_eq!(
        permissions[2],
        json!("core:window:allow-minimize"),
        "capabilities/default.json permissions[2] drifted; packaged shell must retain minimize control"
    );
    assert_eq!(
        permissions[3],
        json!("core:window:allow-toggle-maximize"),
        "capabilities/default.json permissions[3] drifted; packaged shell must retain maximize toggle control"
    );
    assert_eq!(
        permissions[4],
        json!("core:window:allow-close"),
        "capabilities/default.json permissions[4] drifted; packaged shell must retain close control"
    );
    assert_eq!(
        permissions[5],
        json!("dialog:default"),
        "capabilities/default.json permissions[5] drifted; dialog:default is required"
    );
    assert_eq!(
        permissions[6],
        json!({
            "identifier": "opener:allow-open-url",
            "allow": [{ "url": "https://auth.openai.com/*" }]
        }),
        "capabilities/default.json permissions[6] drifted; opener permission must stay scoped to https://auth.openai.com/*"
    );
}

#[test]
fn platform_matrix_artifact_locks_cross_platform_verification_contract() {
    let matrix = include_str!("platform-matrix.md");
    let command = "cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_imported_repo_bridge --test autonomous_fixture_parity --test autonomous_tool_runtime --test runtime_run_persistence --test runtime_supervisor --test runtime_event_stream --test runtime_run_bridge --test notification_route_credentials --test notification_channel_dispatch --test notification_channel_replies --test bootstrap_contracts && pnpm --dir client test -- src/lib/cadence-model.test.ts src/features/cadence/agent-runtime-projections.test.ts src/features/cadence/use-cadence-desktop-state.runtime-run.test.tsx src/features/cadence/live-views.test.tsx components/cadence/agent-runtime.test.tsx src/App.test.tsx && pnpm --dir client exec tauri build --debug";

    assert!(
        matrix.contains(command),
        "platform matrix artifact must lock the exact slice verification command set"
    );
    assert!(matrix.contains("## macOS"));
    assert!(matrix.contains("## Linux"));
    assert!(matrix.contains("## Windows"));
    assert!(matrix.contains("No platform-specific skips"));
}

#[test]
fn serialization_stays_camel_case_for_responses_events_and_errors() {
    let project = sample_project();
    let repository = sample_repository();

    let import_response =
        serde_json::to_value(cadence_desktop_lib::commands::ImportRepositoryResponseDto {
            project: project.clone(),
            repository: repository.clone(),
        })
        .expect("import response should serialize");
    assert_eq!(
        import_response,
        json!({
            "project": {
                "id": "project-1",
                "name": "cadence",
                "description": "Desktop shell",
                "milestone": "M001",
                "totalPhases": 5,
                "completedPhases": 1,
                "activePhase": 2,
                "branch": "main",
                "runtime": null
            },
            "repository": {
                "id": "repo-1",
                "projectId": "project-1",
                "rootPath": "/tmp/cadence",
                "displayName": "cadence",
                "branch": "main",
                "headSha": "abc123",
                "isGitRepo": true
            }
        })
    );

    let snapshot_response = serde_json::to_value(sample_snapshot())
        .expect("project snapshot response should serialize");
    assert_eq!(
        snapshot_response,
        json!({
            "project": {
                "id": "project-1",
                "name": "cadence",
                "description": "Desktop shell",
                "milestone": "M001",
                "totalPhases": 5,
                "completedPhases": 1,
                "activePhase": 2,
                "branch": "main",
                "runtime": null
            },
            "repository": {
                "id": "repo-1",
                "projectId": "project-1",
                "rootPath": "/tmp/cadence",
                "displayName": "cadence",
                "branch": "main",
                "headSha": "abc123",
                "isGitRepo": true
            },
            "phases": [
                {
                    "id": 2,
                    "name": "Live state",
                    "description": "Connect the desktop shell",
                    "status": "active",
                    "currentStep": "execute",
                    "taskCount": 2,
                    "completedTasks": 1,
                    "summary": null
                }
            ],
            "lifecycle": {
                "stages": [
                    {
                        "stage": "discussion",
                        "nodeId": "discussion",
                        "status": "active",
                        "actionRequired": true,
                        "lastTransitionAt": "2026-04-13T20:00:49Z"
                    }
                ]
            },
            "approvalRequests": [
                {
                    "actionId": "review_worktree",
                    "sessionId": "session-1",
                    "flowId": "flow-1",
                    "actionType": "review_worktree",
                    "title": "Repository has local changes",
                    "detail": "cadence currently has 3 changed file(s). Review the worktree before trusting subsequent agent actions.",
                    "gateNodeId": null,
                    "gateKey": null,
                    "transitionFromNodeId": null,
                    "transitionToNodeId": null,
                    "transitionKind": null,
                    "userAnswer": null,
                    "status": "pending",
                    "decisionNote": null,
                    "createdAt": "2026-04-13T20:00:49Z",
                    "updatedAt": "2026-04-13T20:00:49Z",
                    "resolvedAt": null
                }
            ],
            "verificationRecords": [
                {
                    "id": 7,
                    "sourceActionId": "review_worktree",
                    "status": "passed",
                    "summary": "Reviewed repository status before resume.",
                    "detail": "Worktree inspection completed without blocking changes.",
                    "recordedAt": "2026-04-13T20:05:12Z"
                }
            ],
            "resumeHistory": [
                {
                    "id": 4,
                    "sourceActionId": "review_worktree",
                    "sessionId": "session-1",
                    "status": "started",
                    "summary": "Operator resumed the selected project's runtime session.",
                    "createdAt": "2026-04-13T20:06:33Z"
                }
            ],
            "handoffPackages": [
                {
                    "id": 21,
                    "projectId": "project-1",
                    "handoffTransitionId": "auto:txn-002:workflow-discussion:workflow-research",
                    "causalTransitionId": "txn-001",
                    "fromNodeId": "workflow-discussion",
                    "toNodeId": "workflow-research",
                    "transitionKind": "advance",
                    "packagePayload": "{\"schemaVersion\":1,\"triggerTransition\":{\"transitionId\":\"auto:txn-002:workflow-discussion:workflow-research\"}}",
                    "packageHash": "a18fc57e3d2b8f4ef67f5b50f37ba7d85f49a1be987f17fa9dc0ad5a64ff8322",
                    "createdAt": "2026-04-15T18:01:01Z"
                }
            ],
            "autonomousRun": {
                "projectId": "project-1",
                "runId": "run-1",
                "runtimeKind": "openai_codex",
                "providerId": "openai_codex",
                "supervisorKind": "detached_pty",
                "status": "stale",
                "recoveryState": "recovery_required",
                "activeUnitId": "run-1:unit:researcher",
                "activeAttemptId": "run-1:unit:researcher:attempt:1",
                "duplicateStartDetected": false,
                "duplicateStartRunId": null,
                "duplicateStartReason": null,
                "startedAt": "2026-04-15T23:10:00Z",
                "lastHeartbeatAt": "2026-04-15T23:10:01Z",
                "lastCheckpointAt": "2026-04-15T23:10:02Z",
                "pausedAt": null,
                "cancelledAt": null,
                "completedAt": null,
                "crashedAt": "2026-04-15T23:10:03Z",
                "stoppedAt": null,
                "pauseReason": null,
                "cancelReason": null,
                "crashReason": {
                    "code": "runtime_supervisor_connect_failed",
                    "message": "Cadence could not connect to the detached supervisor control endpoint."
                },
                "lastErrorCode": "runtime_supervisor_connect_failed",
                "lastError": {
                    "code": "runtime_supervisor_connect_failed",
                    "message": "Cadence could not connect to the detached supervisor control endpoint."
                },
                "updatedAt": "2026-04-15T23:10:03Z"
            },
            "autonomousUnit": {
                "projectId": "project-1",
                "runId": "run-1",
                "unitId": "run-1:unit:researcher",
                "sequence": 1,
                "kind": "researcher",
                "status": "active",
                "summary": "Researcher child session launched.",
                "boundaryId": null,
                "workflowLinkage": {
                    "workflowNodeId": "workflow-research",
                    "transitionId": "auto:txn-002:workflow-discussion:workflow-research",
                    "causalTransitionId": "txn-001",
                    "handoffTransitionId": "auto:txn-002:workflow-discussion:workflow-research",
                    "handoffPackageHash": "a18fc57e3d2b8f4ef67f5b50f37ba7d85f49a1be987f17fa9dc0ad5a64ff8322"
                },
                "startedAt": "2026-04-15T23:10:00Z",
                "finishedAt": null,
                "updatedAt": "2026-04-15T23:10:03Z",
                "lastErrorCode": "runtime_supervisor_connect_failed",
                "lastError": {
                    "code": "runtime_supervisor_connect_failed",
                    "message": "Cadence could not connect to the detached supervisor control endpoint."
                }
            }
        })
    );

    let project_updated = serde_json::to_value(ProjectUpdatedPayloadDto {
        project: project.clone(),
        reason: ProjectUpdateReason::Imported,
    })
    .expect("project updated payload should serialize");
    assert_eq!(
        project_updated,
        json!({
            "project": {
                "id": "project-1",
                "name": "cadence",
                "description": "Desktop shell",
                "milestone": "M001",
                "totalPhases": 5,
                "completedPhases": 1,
                "activePhase": 2,
                "branch": "main",
                "runtime": null
            },
            "reason": "imported"
        })
    );

    let status_payload = serde_json::to_value(RepositoryStatusChangedPayloadDto {
        project_id: "project-1".into(),
        repository_id: "repo-1".into(),
        status: RepositoryStatusResponseDto {
            repository: repository.clone(),
            branch: Some(BranchSummaryDto {
                name: "main".into(),
                head_sha: Some("abc123".into()),
                detached: false,
            }),
            entries: vec![RepositoryStatusEntryDto {
                path: "src/lib.rs".into(),
                staged: Some(ChangeKind::Modified),
                unstaged: None,
                untracked: false,
            }],
            has_staged_changes: true,
            has_unstaged_changes: false,
            has_untracked_changes: false,
        },
    })
    .expect("status payload should serialize");
    assert_eq!(
        status_payload,
        json!({
            "projectId": "project-1",
            "repositoryId": "repo-1",
            "status": {
                "repository": {
                    "id": "repo-1",
                    "projectId": "project-1",
                    "rootPath": "/tmp/cadence",
                    "displayName": "cadence",
                    "branch": "main",
                    "headSha": "abc123",
                    "isGitRepo": true
                },
                "branch": {
                    "name": "main",
                    "headSha": "abc123",
                    "detached": false
                },
                "entries": [
                    {
                        "path": "src/lib.rs",
                        "staged": "modified",
                        "unstaged": null,
                        "untracked": false
                    }
                ],
                "hasStagedChanges": true,
                "hasUnstagedChanges": false,
                "hasUntrackedChanges": false
            }
        })
    );

    let diff_response = serde_json::to_value(RepositoryDiffResponseDto {
        repository,
        scope: RepositoryDiffScope::Unstaged,
        patch: String::new(),
        truncated: false,
        base_revision: None,
    })
    .expect("diff response should serialize");
    assert_eq!(
        diff_response,
        json!({
            "repository": {
                "id": "repo-1",
                "projectId": "project-1",
                "rootPath": "/tmp/cadence",
                "displayName": "cadence",
                "branch": "main",
                "headSha": "abc123",
                "isGitRepo": true
            },
            "scope": "unstaged",
            "patch": "",
            "truncated": false,
            "baseRevision": null
        })
    );

    let command_error = serde_json::to_value(CommandError {
        code: "desktop_backend_not_ready".into(),
        class: CommandErrorClass::SystemFault,
        message: "Command import_repository is not available from the desktop backend yet.".into(),
        retryable: false,
    })
    .expect("command error should serialize");
    assert_eq!(
        command_error,
        json!({
            "code": "desktop_backend_not_ready",
            "class": "system_fault",
            "message": "Command import_repository is not available from the desktop backend yet.",
            "retryable": false
        })
    );

    let runtime_run_decode_error = serde_json::to_value(CommandError::system_fault(
        "runtime_run_decode_failed",
        "Cadence could not decode durable runtime-run metadata from /tmp/state.db: Field `status` must be a known runtime-run status, found `bogus_status`.",
    ))
    .expect("runtime-run decode error should serialize");
    assert_eq!(
        runtime_run_decode_error,
        json!({
            "code": "runtime_run_decode_failed",
            "class": "system_fault",
            "message": "Cadence could not decode durable runtime-run metadata from /tmp/state.db: Field `status` must be a known runtime-run status, found `bogus_status`.",
            "retryable": false
        })
    );

    let runtime_probe_connect_error = serde_json::to_value(CommandError::retryable(
        "runtime_supervisor_connect_failed",
        "Cadence could not connect to the detached supervisor control endpoint.",
    ))
    .expect("runtime probe connect error should serialize");
    assert_eq!(
        runtime_probe_connect_error,
        json!({
            "code": "runtime_supervisor_connect_failed",
            "class": "retryable",
            "message": "Cadence could not connect to the detached supervisor control endpoint.",
            "retryable": true
        })
    );

    let runtime_settings_request = serde_json::to_value(
        cadence_desktop_lib::commands::UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: Some("credential-value-1".into()),
        },
    )
    .expect("runtime settings request should serialize");
    assert_eq!(
        runtime_settings_request,
        json!({
            "providerId": "openrouter",
            "modelId": "openai/gpt-4o-mini",
            "openrouterApiKey": "credential-value-1"
        })
    );

    let runtime_settings_response = serde_json::to_value(
        cadence_desktop_lib::commands::RuntimeSettingsDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key_configured: true,
        },
    )
    .expect("runtime settings response should serialize");
    assert_eq!(
        runtime_settings_response,
        json!({
            "providerId": "openrouter",
            "modelId": "openai/gpt-4o-mini",
            "openrouterApiKeyConfigured": true
        })
    );

    let runtime_status = serde_json::to_value(RuntimeAuthStatusDto {
        project_id: "project-1".into(),
        runtime_kind: "openai_codex".into(),
        provider_id: "openai_codex".into(),
        flow_id: Some("flow-1".into()),
        session_id: Some("session-1".into()),
        account_id: Some("acct-1".into()),
        phase: RuntimeAuthPhase::AwaitingManualInput,
        callback_bound: Some(false),
        authorization_url: Some("https://auth.openai.com/oauth/authorize".into()),
        redirect_uri: Some("http://127.0.0.1:1455/auth/callback".into()),
        last_error_code: Some("callback_listener_bind_failed".into()),
        last_error: Some(RuntimeDiagnosticDto {
            code: "callback_listener_bind_failed".into(),
            message: "listener unavailable".into(),
            retryable: false,
        }),
        updated_at: "2026-04-13T14:11:59Z".into(),
    })
    .expect("runtime auth status should serialize");
    assert_eq!(
        runtime_status,
        json!({
            "projectId": "project-1",
            "runtimeKind": "openai_codex",
            "providerId": "openai_codex",
            "flowId": "flow-1",
            "sessionId": "session-1",
            "accountId": "acct-1",
            "phase": "awaiting_manual_input",
            "callbackBound": false,
            "authorizationUrl": "https://auth.openai.com/oauth/authorize",
            "redirectUri": "http://127.0.0.1:1455/auth/callback",
            "lastErrorCode": "callback_listener_bind_failed",
            "lastError": {
                "code": "callback_listener_bind_failed",
                "message": "listener unavailable",
                "retryable": false
            },
            "updatedAt": "2026-04-13T14:11:59Z"
        })
    );

    let runtime_updated = serde_json::to_value(RuntimeUpdatedPayloadDto {
        project_id: "project-1".into(),
        runtime_kind: "openai_codex".into(),
        provider_id: "openai_codex".into(),
        flow_id: Some("flow-1".into()),
        session_id: Some("session-1".into()),
        account_id: Some("acct-1".into()),
        auth_phase: RuntimeAuthPhase::Authenticated,
        last_error_code: None,
        last_error: None,
        updated_at: "2026-04-13T14:11:59Z".into(),
    })
    .expect("runtime updated payload should serialize");
    assert_eq!(
        runtime_updated,
        json!({
            "projectId": "project-1",
            "runtimeKind": "openai_codex",
            "providerId": "openai_codex",
            "flowId": "flow-1",
            "sessionId": "session-1",
            "accountId": "acct-1",
            "authPhase": "authenticated",
            "lastErrorCode": null,
            "lastError": null,
            "updatedAt": "2026-04-13T14:11:59Z"
        })
    );

    let runtime_run = serde_json::to_value(RuntimeRunDto {
        project_id: "project-1".into(),
        run_id: "run-1".into(),
        runtime_kind: "openai_codex".into(),
        provider_id: "openai_codex".into(),
        supervisor_kind: "detached_pty".into(),
        status: RuntimeRunStatusDto::Stale,
        transport: RuntimeRunTransportDto {
            kind: "tcp".into(),
            endpoint: "127.0.0.1:45123".into(),
            liveness: RuntimeRunTransportLivenessDto::Unreachable,
        },
        started_at: "2026-04-15T23:10:00Z".into(),
        last_heartbeat_at: Some("2026-04-15T23:10:01Z".into()),
        last_checkpoint_sequence: 2,
        last_checkpoint_at: Some("2026-04-15T23:10:02Z".into()),
        stopped_at: None,
        last_error_code: Some("runtime_supervisor_connect_failed".into()),
        last_error: Some(RuntimeRunDiagnosticDto {
            code: "runtime_supervisor_connect_failed".into(),
            message: "Cadence could not connect to the detached supervisor control endpoint."
                .into(),
        }),
        updated_at: "2026-04-15T23:10:02Z".into(),
        checkpoints: vec![RuntimeRunCheckpointDto {
            sequence: 2,
            kind: RuntimeRunCheckpointKindDto::State,
            summary: "Supervisor heartbeat recorded.".into(),
            created_at: "2026-04-15T23:10:02Z".into(),
        }],
    })
    .expect("runtime run payload should serialize");
    assert_eq!(
        runtime_run,
        json!({
            "projectId": "project-1",
            "runId": "run-1",
            "runtimeKind": "openai_codex",
            "providerId": "openai_codex",
            "supervisorKind": "detached_pty",
            "status": "stale",
            "transport": {
                "kind": "tcp",
                "endpoint": "127.0.0.1:45123",
                "liveness": "unreachable"
            },
            "startedAt": "2026-04-15T23:10:00Z",
            "lastHeartbeatAt": "2026-04-15T23:10:01Z",
            "lastCheckpointSequence": 2,
            "lastCheckpointAt": "2026-04-15T23:10:02Z",
            "stoppedAt": null,
            "lastErrorCode": "runtime_supervisor_connect_failed",
            "lastError": {
                "code": "runtime_supervisor_connect_failed",
                "message": "Cadence could not connect to the detached supervisor control endpoint."
            },
            "updatedAt": "2026-04-15T23:10:02Z",
            "checkpoints": [
                {
                    "sequence": 2,
                    "kind": "state",
                    "summary": "Supervisor heartbeat recorded.",
                    "createdAt": "2026-04-15T23:10:02Z"
                }
            ]
        })
    );

    let runtime_run_updated = serde_json::to_value(RuntimeRunUpdatedPayloadDto {
        project_id: "project-1".into(),
        run: Some(RuntimeRunDto {
            project_id: "project-1".into(),
            run_id: "run-1".into(),
            runtime_kind: "openai_codex".into(),
            provider_id: "openai_codex".into(),
            supervisor_kind: "detached_pty".into(),
            status: RuntimeRunStatusDto::Running,
            transport: RuntimeRunTransportDto {
                kind: "tcp".into(),
                endpoint: "127.0.0.1:45123".into(),
                liveness: RuntimeRunTransportLivenessDto::Reachable,
            },
            started_at: "2026-04-15T23:10:00Z".into(),
            last_heartbeat_at: Some("2026-04-15T23:10:01Z".into()),
            last_checkpoint_sequence: 2,
            last_checkpoint_at: Some("2026-04-15T23:10:02Z".into()),
            stopped_at: None,
            last_error_code: None,
            last_error: None,
            updated_at: "2026-04-15T23:10:02Z".into(),
            checkpoints: vec![],
        }),
    })
    .expect("runtime run updated payload should serialize");
    assert_eq!(
        runtime_run_updated,
        json!({
            "projectId": "project-1",
            "run": {
                "projectId": "project-1",
                "runId": "run-1",
                "runtimeKind": "openai_codex",
                "providerId": "openai_codex",
                "supervisorKind": "detached_pty",
                "status": "running",
                "transport": {
                    "kind": "tcp",
                    "endpoint": "127.0.0.1:45123",
                    "liveness": "reachable"
                },
                "startedAt": "2026-04-15T23:10:00Z",
                "lastHeartbeatAt": "2026-04-15T23:10:01Z",
                "lastCheckpointSequence": 2,
                "lastCheckpointAt": "2026-04-15T23:10:02Z",
                "stoppedAt": null,
                "lastErrorCode": null,
                "lastError": null,
                "updatedAt": "2026-04-15T23:10:02Z",
                "checkpoints": []
            }
        })
    );

    let autonomous_state = serde_json::to_value(AutonomousRunStateDto {
        run: Some(sample_autonomous_run(true)),
        unit: Some(sample_autonomous_unit()),
        attempt: Some(sample_autonomous_attempt()),
        history: sample_autonomous_history(),
    })
    .expect("autonomous run state should serialize");
    assert_eq!(
        autonomous_state,
        json!({
            "run": {
                "projectId": "project-1",
                "runId": "run-1",
                "runtimeKind": "openai_codex",
                "providerId": "openai_codex",
                "supervisorKind": "detached_pty",
                "status": "stale",
                "recoveryState": "recovery_required",
                "activeUnitId": "run-1:unit:researcher",
                "activeAttemptId": "run-1:unit:researcher:attempt:1",
                "duplicateStartDetected": true,
                "duplicateStartRunId": "run-1",
                "duplicateStartReason": "Cadence reused the already-active autonomous run for this project instead of launching a duplicate supervisor.",
                "startedAt": "2026-04-15T23:10:00Z",
                "lastHeartbeatAt": "2026-04-15T23:10:01Z",
                "lastCheckpointAt": "2026-04-15T23:10:02Z",
                "pausedAt": null,
                "cancelledAt": null,
                "completedAt": null,
                "crashedAt": "2026-04-15T23:10:03Z",
                "stoppedAt": null,
                "pauseReason": null,
                "cancelReason": null,
                "crashReason": {
                    "code": "runtime_supervisor_connect_failed",
                    "message": "Cadence could not connect to the detached supervisor control endpoint."
                },
                "lastErrorCode": "runtime_supervisor_connect_failed",
                "lastError": {
                    "code": "runtime_supervisor_connect_failed",
                    "message": "Cadence could not connect to the detached supervisor control endpoint."
                },
                "updatedAt": "2026-04-15T23:10:03Z"
            },
            "unit": {
                "projectId": "project-1",
                "runId": "run-1",
                "unitId": "run-1:unit:researcher",
                "sequence": 1,
                "kind": "researcher",
                "status": "active",
                "summary": "Researcher child session launched.",
                "boundaryId": null,
                "workflowLinkage": {
                    "workflowNodeId": "workflow-research",
                    "transitionId": "auto:txn-002:workflow-discussion:workflow-research",
                    "causalTransitionId": "txn-001",
                    "handoffTransitionId": "auto:txn-002:workflow-discussion:workflow-research",
                    "handoffPackageHash": "a18fc57e3d2b8f4ef67f5b50f37ba7d85f49a1be987f17fa9dc0ad5a64ff8322"
                },
                "startedAt": "2026-04-15T23:10:00Z",
                "finishedAt": null,
                "updatedAt": "2026-04-15T23:10:03Z",
                "lastErrorCode": "runtime_supervisor_connect_failed",
                "lastError": {
                    "code": "runtime_supervisor_connect_failed",
                    "message": "Cadence could not connect to the detached supervisor control endpoint."
                }
            },
            "attempt": {
                "projectId": "project-1",
                "runId": "run-1",
                "unitId": "run-1:unit:researcher",
                "attemptId": "run-1:unit:researcher:attempt:1",
                "attemptNumber": 1,
                "childSessionId": "child-session-1",
                "status": "active",
                "boundaryId": null,
                "workflowLinkage": {
                    "workflowNodeId": "workflow-research",
                    "transitionId": "auto:txn-002:workflow-discussion:workflow-research",
                    "causalTransitionId": "txn-001",
                    "handoffTransitionId": "auto:txn-002:workflow-discussion:workflow-research",
                    "handoffPackageHash": "a18fc57e3d2b8f4ef67f5b50f37ba7d85f49a1be987f17fa9dc0ad5a64ff8322"
                },
                "startedAt": "2026-04-15T23:10:00Z",
                "finishedAt": null,
                "updatedAt": "2026-04-15T23:10:03Z",
                "lastErrorCode": "runtime_supervisor_connect_failed",
                "lastError": {
                    "code": "runtime_supervisor_connect_failed",
                    "message": "Cadence could not connect to the detached supervisor control endpoint."
                }
            },
            "history": [
                {
                    "unit": {
                        "projectId": "project-1",
                        "runId": "run-1",
                        "unitId": "run-1:unit:researcher",
                        "sequence": 1,
                        "kind": "researcher",
                        "status": "active",
                        "summary": "Researcher child session launched.",
                        "boundaryId": null,
                        "workflowLinkage": {
                            "workflowNodeId": "workflow-research",
                            "transitionId": "auto:txn-002:workflow-discussion:workflow-research",
                            "causalTransitionId": "txn-001",
                            "handoffTransitionId": "auto:txn-002:workflow-discussion:workflow-research",
                            "handoffPackageHash": "a18fc57e3d2b8f4ef67f5b50f37ba7d85f49a1be987f17fa9dc0ad5a64ff8322"
                        },
                        "startedAt": "2026-04-15T23:10:00Z",
                        "finishedAt": null,
                        "updatedAt": "2026-04-15T23:10:03Z",
                        "lastErrorCode": "runtime_supervisor_connect_failed",
                        "lastError": {
                            "code": "runtime_supervisor_connect_failed",
                            "message": "Cadence could not connect to the detached supervisor control endpoint."
                        }
                    },
                    "latestAttempt": {
                        "projectId": "project-1",
                        "runId": "run-1",
                        "unitId": "run-1:unit:researcher",
                        "attemptId": "run-1:unit:researcher:attempt:1",
                        "attemptNumber": 1,
                        "childSessionId": "child-session-1",
                        "status": "active",
                        "boundaryId": null,
                        "workflowLinkage": {
                            "workflowNodeId": "workflow-research",
                            "transitionId": "auto:txn-002:workflow-discussion:workflow-research",
                            "causalTransitionId": "txn-001",
                            "handoffTransitionId": "auto:txn-002:workflow-discussion:workflow-research",
                            "handoffPackageHash": "a18fc57e3d2b8f4ef67f5b50f37ba7d85f49a1be987f17fa9dc0ad5a64ff8322"
                        },
                        "startedAt": "2026-04-15T23:10:00Z",
                        "finishedAt": null,
                        "updatedAt": "2026-04-15T23:10:03Z",
                        "lastErrorCode": "runtime_supervisor_connect_failed",
                        "lastError": {
                            "code": "runtime_supervisor_connect_failed",
                            "message": "Cadence could not connect to the detached supervisor control endpoint."
                        }
                    },
                    "artifacts": [
                        {
                            "projectId": "project-1",
                            "runId": "run-1",
                            "unitId": "run-1:unit:researcher",
                            "attemptId": "run-1:unit:researcher:attempt:1",
                            "artifactId": "artifact-tool-result",
                            "artifactKind": "tool_result",
                            "status": "recorded",
                            "summary": "Shell tool result persisted for downstream verification.",
                            "contentHash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                            "payload": {
                                "kind": "tool_result",
                                "projectId": "project-1",
                                "runId": "run-1",
                                "unitId": "run-1:unit:researcher",
                                "attemptId": "run-1:unit:researcher:attempt:1",
                                "artifactId": "artifact-tool-result",
                                "toolCallId": "tool-call-1",
                                "toolName": "shell.exec",
                                "toolState": "succeeded",
                                "commandResult": {
                                    "exitCode": 0,
                                    "timedOut": false,
                                    "summary": "Command exited successfully after persisting structured evidence."
                                },
                                "actionId": "action-1",
                                "boundaryId": "boundary-1"
                            },
                            "createdAt": "2026-04-15T23:10:03Z",
                            "updatedAt": "2026-04-15T23:10:03Z"
                        }
                    ]
                }
            ]
        })
    );

    let start_request = serde_json::to_value(StartOpenAiLoginRequestDto {
        project_id: "project-1".into(),
        originator: Some("cadence-tests".into()),
    })
    .expect("auth start request should serialize");
    assert_eq!(
        start_request,
        json!({ "projectId": "project-1", "originator": "cadence-tests" })
    );

    let complete_request = serde_json::to_value(SubmitOpenAiCallbackRequestDto {
        project_id: "project-1".into(),
        flow_id: "flow-1".into(),
        manual_input: Some("http://localhost:1455/auth/callback?code=abc".into()),
    })
    .expect("auth complete request should serialize");
    assert_eq!(
        complete_request,
        json!({
            "projectId": "project-1",
            "flowId": "flow-1",
            "manualInput": "http://localhost:1455/auth/callback?code=abc"
        })
    );

    let refresh_request = serde_json::to_value(ProjectIdRequestDto {
        project_id: "project-1".into(),
    })
    .expect("auth refresh request should serialize");
    assert_eq!(refresh_request, json!({ "projectId": "project-1" }));

    let get_autonomous_run_request = serde_json::to_value(GetAutonomousRunRequestDto {
        project_id: "project-1".into(),
    })
    .expect("get autonomous run request should serialize");
    assert_eq!(
        get_autonomous_run_request,
        json!({ "projectId": "project-1" })
    );

    let get_runtime_run_request = serde_json::to_value(GetRuntimeRunRequestDto {
        project_id: "project-1".into(),
    })
    .expect("get runtime run request should serialize");
    assert_eq!(get_runtime_run_request, json!({ "projectId": "project-1" }));

    let start_autonomous_run_request = serde_json::to_value(StartAutonomousRunRequestDto {
        project_id: "project-1".into(),
    })
    .expect("start autonomous run request should serialize");
    assert_eq!(
        start_autonomous_run_request,
        json!({ "projectId": "project-1" })
    );

    let start_runtime_run_request = serde_json::to_value(StartRuntimeRunRequestDto {
        project_id: "project-1".into(),
    })
    .expect("start runtime run request should serialize");
    assert_eq!(
        start_runtime_run_request,
        json!({ "projectId": "project-1" })
    );

    let cancel_autonomous_run_request = serde_json::to_value(CancelAutonomousRunRequestDto {
        project_id: "project-1".into(),
        run_id: "run-1".into(),
    })
    .expect("cancel autonomous run request should serialize");
    assert_eq!(
        cancel_autonomous_run_request,
        json!({ "projectId": "project-1", "runId": "run-1" })
    );

    let stop_runtime_run_request = serde_json::to_value(StopRuntimeRunRequestDto {
        project_id: "project-1".into(),
        run_id: "run-1".into(),
    })
    .expect("stop runtime run request should serialize");
    assert_eq!(
        stop_runtime_run_request,
        json!({ "projectId": "project-1", "runId": "run-1" })
    );

    let resolve_request = serde_json::to_value(ResolveOperatorActionRequestDto {
        project_id: "project-1".into(),
        action_id: "session:session-1:review_worktree".into(),
        decision: "approve".into(),
        user_answer: Some("Worktree reviewed and accepted.".into()),
    })
    .expect("resolve operator action request should serialize");
    assert_eq!(
        resolve_request,
        json!({
            "projectId": "project-1",
            "actionId": "session:session-1:review_worktree",
            "decision": "approve",
            "userAnswer": "Worktree reviewed and accepted."
        })
    );

    let resolve_response = serde_json::to_value(ResolveOperatorActionResponseDto {
        approval_request: OperatorApprovalDto {
            action_id: "session:session-1:review_worktree".into(),
            session_id: Some("session-1".into()),
            flow_id: Some("flow-1".into()),
            action_type: "review_worktree".into(),
            title: "Repository has local changes".into(),
            detail: "cadence currently has 3 changed file(s). Review the worktree before trusting subsequent agent actions.".into(),
            gate_node_id: None,
            gate_key: None,
            transition_from_node_id: None,
            transition_to_node_id: None,
            transition_kind: None,
            user_answer: Some("Worktree reviewed and accepted.".into()),
            status: OperatorApprovalStatus::Approved,
            decision_note: Some("Worktree reviewed and accepted.".into()),
            created_at: "2026-04-13T20:00:49Z".into(),
            updated_at: "2026-04-13T20:04:19Z".into(),
            resolved_at: Some("2026-04-13T20:04:19Z".into()),
        },
        verification_record: VerificationRecordDto {
            id: 8,
            source_action_id: Some("session:session-1:review_worktree".into()),
            status: VerificationRecordStatus::Passed,
            summary: "Approved operator action: Repository has local changes.".into(),
            detail: Some("Worktree reviewed and accepted.".into()),
            recorded_at: "2026-04-13T20:04:19Z".into(),
        },
    })
    .expect("resolve operator action response should serialize");
    assert_eq!(
        resolve_response,
        json!({
            "approvalRequest": {
                "actionId": "session:session-1:review_worktree",
                "sessionId": "session-1",
                "flowId": "flow-1",
                "actionType": "review_worktree",
                "title": "Repository has local changes",
                "detail": "cadence currently has 3 changed file(s). Review the worktree before trusting subsequent agent actions.",
                "gateNodeId": null,
                "gateKey": null,
                "transitionFromNodeId": null,
                "transitionToNodeId": null,
                "transitionKind": null,
                "userAnswer": "Worktree reviewed and accepted.",
                "status": "approved",
                "decisionNote": "Worktree reviewed and accepted.",
                "createdAt": "2026-04-13T20:00:49Z",
                "updatedAt": "2026-04-13T20:04:19Z",
                "resolvedAt": "2026-04-13T20:04:19Z"
            },
            "verificationRecord": {
                "id": 8,
                "sourceActionId": "session:session-1:review_worktree",
                "status": "passed",
                "summary": "Approved operator action: Repository has local changes.",
                "detail": "Worktree reviewed and accepted.",
                "recordedAt": "2026-04-13T20:04:19Z"
            }
        })
    );

    let resume_request = serde_json::to_value(ResumeOperatorRunRequestDto {
        project_id: "project-1".into(),
        action_id: "session:session-1:review_worktree".into(),
        user_answer: Some("Worktree reviewed and accepted.".into()),
    })
    .expect("resume operator run request should serialize");
    assert_eq!(
        resume_request,
        json!({
            "projectId": "project-1",
            "actionId": "session:session-1:review_worktree",
            "userAnswer": "Worktree reviewed and accepted."
        })
    );

    let resume_response = serde_json::to_value(ResumeOperatorRunResponseDto {
        approval_request: OperatorApprovalDto {
            action_id: "session:session-1:review_worktree".into(),
            session_id: Some("session-1".into()),
            flow_id: Some("flow-1".into()),
            action_type: "review_worktree".into(),
            title: "Repository has local changes".into(),
            detail: "cadence currently has 3 changed file(s). Review the worktree before trusting subsequent agent actions.".into(),
            gate_node_id: None,
            gate_key: None,
            transition_from_node_id: None,
            transition_to_node_id: None,
            transition_kind: None,
            user_answer: Some("Worktree reviewed and accepted.".into()),
            status: OperatorApprovalStatus::Approved,
            decision_note: Some("Worktree reviewed and accepted.".into()),
            created_at: "2026-04-13T20:00:49Z".into(),
            updated_at: "2026-04-13T20:04:19Z".into(),
            resolved_at: Some("2026-04-13T20:04:19Z".into()),
        },
        resume_entry: ResumeHistoryEntryDto {
            id: 5,
            source_action_id: Some("session:session-1:review_worktree".into()),
            session_id: Some("session-1".into()),
            status: ResumeHistoryStatus::Started,
            summary: "Operator resumed the selected project's runtime session after approving Repository has local changes.".into(),
            created_at: "2026-04-13T20:06:33Z".into(),
        },
        automatic_dispatch: Some(sample_automatic_dispatch_outcome()),
    })
    .expect("resume operator run response should serialize");
    assert_eq!(
        resume_response,
        json!({
            "approvalRequest": {
                "actionId": "session:session-1:review_worktree",
                "sessionId": "session-1",
                "flowId": "flow-1",
                "actionType": "review_worktree",
                "title": "Repository has local changes",
                "detail": "cadence currently has 3 changed file(s). Review the worktree before trusting subsequent agent actions.",
                "gateNodeId": null,
                "gateKey": null,
                "transitionFromNodeId": null,
                "transitionToNodeId": null,
                "transitionKind": null,
                "userAnswer": "Worktree reviewed and accepted.",
                "status": "approved",
                "decisionNote": "Worktree reviewed and accepted.",
                "createdAt": "2026-04-13T20:00:49Z",
                "updatedAt": "2026-04-13T20:04:19Z",
                "resolvedAt": "2026-04-13T20:04:19Z"
            },
            "resumeEntry": {
                "id": 5,
                "sourceActionId": "session:session-1:review_worktree",
                "sessionId": "session-1",
                "status": "started",
                "summary": "Operator resumed the selected project's runtime session after approving Repository has local changes.",
                "createdAt": "2026-04-13T20:06:33Z"
            },
            "automaticDispatch": {
                "status": "applied",
                "transitionEvent": {
                    "id": 12,
                    "transitionId": "txn-002",
                    "causalTransitionId": "txn-001",
                    "fromNodeId": "plan",
                    "toNodeId": "execute",
                    "transitionKind": "advance",
                    "gateDecision": "approved",
                    "gateDecisionContext": "operator-approved",
                    "createdAt": "2026-04-15T18:01:00Z"
                },
                "handoffPackage": {
                    "status": "persisted",
                    "package": {
                        "id": 21,
                        "projectId": "project-1",
                        "handoffTransitionId": "auto:txn-002:workflow-discussion:workflow-research",
                        "causalTransitionId": "txn-001",
                        "fromNodeId": "workflow-discussion",
                        "toNodeId": "workflow-research",
                        "transitionKind": "advance",
                        "packagePayload": "{\"schemaVersion\":1,\"triggerTransition\":{\"transitionId\":\"auto:txn-002:workflow-discussion:workflow-research\"}}",
                        "packageHash": "a18fc57e3d2b8f4ef67f5b50f37ba7d85f49a1be987f17fa9dc0ad5a64ff8322",
                        "createdAt": "2026-04-15T18:01:01Z"
                    },
                    "code": null,
                    "message": null
                },
                "code": null,
                "message": null
            }
        })
    );
    let list_notification_routes_request = serde_json::to_value(ListNotificationRoutesRequestDto {
        project_id: "project-1".into(),
    })
    .expect("list notification routes request should serialize");
    assert_eq!(
        list_notification_routes_request,
        json!({
            "projectId": "project-1"
        })
    );

    let upsert_notification_route_request =
        serde_json::to_value(UpsertNotificationRouteRequestDto {
            project_id: "project-1".into(),
            route_id: "route-discord".into(),
            route_kind: "discord".into(),
            route_target: "123456789012345678".into(),
            enabled: true,
            metadata_json: Some("{\"channel\":\"ops\"}".into()),
            updated_at: "2026-04-16T20:06:33Z".into(),
        })
        .expect("upsert notification route request should serialize");
    assert_eq!(
        upsert_notification_route_request,
        json!({
            "projectId": "project-1",
            "routeId": "route-discord",
            "routeKind": "discord",
            "routeTarget": "123456789012345678",
            "enabled": true,
            "metadataJson": "{\"channel\":\"ops\"}",
            "updatedAt": "2026-04-16T20:06:33Z"
        })
    );

    let list_notification_routes_response =
        serde_json::to_value(ListNotificationRoutesResponseDto {
            routes: vec![NotificationRouteDto {
                project_id: "project-1".into(),
                route_id: "route-discord".into(),
                route_kind: NotificationRouteKindDto::Discord,
                route_target: "123456789012345678".into(),
                enabled: true,
                metadata_json: Some("{\"channel\":\"ops\"}".into()),
                credential_readiness: Some(cadence_desktop_lib::commands::NotificationRouteCredentialReadinessDto {
                    has_bot_token: false,
                    has_chat_id: false,
                    has_webhook_url: true,
                    ready: false,
                    status:
                        cadence_desktop_lib::commands::NotificationRouteCredentialReadinessStatusDto::Missing,
                    diagnostic: Some(
                        cadence_desktop_lib::commands::NotificationRouteCredentialReadinessDiagnosticDto {
                            code: "notification_adapter_credentials_missing".into(),
                            message:
                                "Cadence requires app-local `webhookUrl` and `botToken` credentials for Discord route `route-discord` in project `project-1` to support autonomous dispatch + reply handling.".into(),
                            retryable: false,
                        },
                    ),
                }),
                created_at: "2026-04-16T20:05:30Z".into(),
                updated_at: "2026-04-16T20:06:33Z".into(),
            }],
        })
        .expect("list notification routes response should serialize");
    assert_eq!(
        list_notification_routes_response,
        json!({
            "routes": [
                {
                    "projectId": "project-1",
                    "routeId": "route-discord",
                    "routeKind": "discord",
                    "routeTarget": "123456789012345678",
                    "enabled": true,
                    "metadataJson": "{\"channel\":\"ops\"}",
                    "credentialReadiness": {
                        "hasBotToken": false,
                        "hasChatId": false,
                        "hasWebhookUrl": true,
                        "ready": false,
                        "status": "missing",
                        "diagnostic": {
                            "code": "notification_adapter_credentials_missing",
                            "message": "Cadence requires app-local `webhookUrl` and `botToken` credentials for Discord route `route-discord` in project `project-1` to support autonomous dispatch + reply handling.",
                            "retryable": false
                        }
                    },
                    "createdAt": "2026-04-16T20:05:30Z",
                    "updatedAt": "2026-04-16T20:06:33Z"
                }
            ]
        })
    );

    let upsert_notification_route_response =
        serde_json::to_value(UpsertNotificationRouteResponseDto {
            route: NotificationRouteDto {
                project_id: "project-1".into(),
                route_id: "route-discord".into(),
                route_kind: NotificationRouteKindDto::Discord,
                route_target: "123456789012345678".into(),
                enabled: true,
                metadata_json: Some("{\"channel\":\"ops\"}".into()),
                credential_readiness: None,
                created_at: "2026-04-16T20:05:30Z".into(),
                updated_at: "2026-04-16T20:06:33Z".into(),
            },
        })
        .expect("upsert notification route response should serialize");
    assert_eq!(
        upsert_notification_route_response,
        json!({
            "route": {
                "projectId": "project-1",
                "routeId": "route-discord",
                "routeKind": "discord",
                "routeTarget": "123456789012345678",
                "enabled": true,
                "metadataJson": "{\"channel\":\"ops\"}",
                "credentialReadiness": null,
                "createdAt": "2026-04-16T20:05:30Z",
                "updatedAt": "2026-04-16T20:06:33Z"
            }
        })
    );

    let upsert_notification_route_credentials_request =
        serde_json::to_value(UpsertNotificationRouteCredentialsRequestDto {
            project_id: "project-1".into(),
            route_id: "route-discord".into(),
            route_kind: "discord".into(),
            credentials: cadence_desktop_lib::commands::NotificationRouteCredentialPayloadDto {
                bot_token: Some("discord-bot-token".into()),
                chat_id: None,
                webhook_url: Some("https://discord.com/api/webhooks/1/2".into()),
            },
            updated_at: "2026-04-16T20:06:33Z".into(),
        })
        .expect("upsert notification route credentials request should serialize");
    assert_eq!(
        upsert_notification_route_credentials_request,
        json!({
            "projectId": "project-1",
            "routeId": "route-discord",
            "routeKind": "discord",
            "credentials": {
                "botToken": "discord-bot-token",
                "chatId": null,
                "webhookUrl": "https://discord.com/api/webhooks/1/2"
            },
            "updatedAt": "2026-04-16T20:06:33Z"
        })
    );

    let upsert_notification_route_credentials_response =
        serde_json::to_value(UpsertNotificationRouteCredentialsResponseDto {
            project_id: "project-1".into(),
            route_id: "route-discord".into(),
            route_kind: NotificationRouteKindDto::Discord,
            credential_scope: "app_local".into(),
            has_bot_token: true,
            has_chat_id: false,
            has_webhook_url: true,
            updated_at: "2026-04-16T20:06:33Z".into(),
        })
        .expect("upsert notification route credentials response should serialize");
    assert_eq!(
        upsert_notification_route_credentials_response,
        json!({
            "projectId": "project-1",
            "routeId": "route-discord",
            "routeKind": "discord",
            "credentialScope": "app_local",
            "hasBotToken": true,
            "hasChatId": false,
            "hasWebhookUrl": true,
            "updatedAt": "2026-04-16T20:06:33Z"
        })
    );

    let list_dispatches_request = serde_json::to_value(ListNotificationDispatchesRequestDto {
        project_id: "project-1".into(),
        action_id: Some("session:session-1:review_worktree".into()),
    })
    .expect("list notification dispatches request should serialize");
    assert_eq!(
        list_dispatches_request,
        json!({
            "projectId": "project-1",
            "actionId": "session:session-1:review_worktree"
        })
    );

    let record_dispatch_outcome_request =
        serde_json::to_value(RecordNotificationDispatchOutcomeRequestDto {
            project_id: "project-1".into(),
            action_id: "session:session-1:review_worktree".into(),
            route_id: "route-discord".into(),
            status: NotificationDispatchOutcomeStatusDto::Sent,
            attempted_at: "2026-04-16T20:06:33Z".into(),
            error_code: None,
            error_message: None,
        })
        .expect("record notification dispatch outcome request should serialize");
    assert_eq!(
        record_dispatch_outcome_request,
        json!({
            "projectId": "project-1",
            "actionId": "session:session-1:review_worktree",
            "routeId": "route-discord",
            "status": "sent",
            "attemptedAt": "2026-04-16T20:06:33Z",
            "errorCode": null,
            "errorMessage": null
        })
    );

    let submit_notification_reply_request =
        serde_json::to_value(SubmitNotificationReplyRequestDto {
            project_id: "project-1".into(),
            action_id: "session:session-1:review_worktree".into(),
            route_id: "route-discord".into(),
            correlation_key: "nfy:11111111111111111111111111111111".into(),
            responder_id: Some("operator-a".into()),
            reply_text: "Looks good".into(),
            decision: "approve".into(),
            received_at: "2026-04-16T20:06:34Z".into(),
        })
        .expect("submit notification reply request should serialize");
    assert_eq!(
        submit_notification_reply_request,
        json!({
            "projectId": "project-1",
            "actionId": "session:session-1:review_worktree",
            "routeId": "route-discord",
            "correlationKey": "nfy:11111111111111111111111111111111",
            "responderId": "operator-a",
            "replyText": "Looks good",
            "decision": "approve",
            "receivedAt": "2026-04-16T20:06:34Z"
        })
    );

    let sync_notification_adapters_request =
        serde_json::to_value(SyncNotificationAdaptersRequestDto {
            project_id: "project-1".into(),
        })
        .expect("sync notification adapters request should serialize");
    assert_eq!(
        sync_notification_adapters_request,
        json!({
            "projectId": "project-1"
        })
    );

    let sync_notification_adapters_response =
        serde_json::to_value(SyncNotificationAdaptersResponseDto {
            project_id: "project-1".into(),
            dispatch: cadence_desktop_lib::commands::NotificationDispatchCycleSummaryDto {
                project_id: "project-1".into(),
                pending_count: 2,
                attempted_count: 2,
                sent_count: 1,
                failed_count: 1,
                attempt_limit: 64,
                attempts_truncated: false,
                attempts: vec![cadence_desktop_lib::commands::NotificationAdapterDispatchAttemptDto {
                    dispatch_id: 11,
                    action_id: "session:session-1:review_worktree".into(),
                    route_id: "route-discord".into(),
                    route_kind: "discord".into(),
                    outcome_status: NotificationDispatchStatusDto::Sent,
                    diagnostic_code: "notification_adapter_dispatch_attempted".into(),
                    diagnostic_message:
                        "Cadence sent notification dispatch `11` for route `route-discord`."
                            .into(),
                    durable_error_code: None,
                    durable_error_message: None,
                }],
                error_code_counts: vec![
                    cadence_desktop_lib::commands::NotificationAdapterErrorCountDto {
                        code: "notification_adapter_transport_timeout".into(),
                        count: 1,
                    },
                ],
            },
            replies: cadence_desktop_lib::commands::NotificationReplyCycleSummaryDto {
                project_id: "project-1".into(),
                route_count: 2,
                polled_route_count: 2,
                message_count: 2,
                accepted_count: 1,
                rejected_count: 1,
                attempt_limit: 256,
                attempts_truncated: false,
                attempts: vec![cadence_desktop_lib::commands::NotificationAdapterReplyAttemptDto {
                    route_id: "route-discord".into(),
                    route_kind: "discord".into(),
                    action_id: Some("session:session-1:review_worktree".into()),
                    message_id: Some("42".into()),
                    accepted: true,
                    diagnostic_code: "notification_adapter_reply_received".into(),
                    diagnostic_message:
                        "Cadence accepted inbound reply for route `route-discord` and resumed the correlated action through the existing broker path."
                            .into(),
                    reply_code: None,
                    reply_message: None,
                }],
                error_code_counts: vec![
                    cadence_desktop_lib::commands::NotificationAdapterErrorCountDto {
                        code: "notification_reply_already_claimed".into(),
                        count: 1,
                    },
                ],
            },
            synced_at: "2026-04-16T20:06:35Z".into(),
        })
        .expect("sync notification adapters response should serialize");
    assert_eq!(
        sync_notification_adapters_response["projectId"],
        json!("project-1")
    );
    assert_eq!(
        sync_notification_adapters_response["dispatch"]["attemptedCount"],
        json!(2)
    );
    assert_eq!(
        sync_notification_adapters_response["replies"]["acceptedCount"],
        json!(1)
    );

    let list_dispatches_response = serde_json::to_value(ListNotificationDispatchesResponseDto {
        dispatches: vec![NotificationDispatchDto {
            id: 1,
            project_id: "project-1".into(),
            action_id: "session:session-1:review_worktree".into(),
            route_id: "route-discord".into(),
            correlation_key: "nfy:11111111111111111111111111111111".into(),
            status: NotificationDispatchStatusDto::Pending,
            attempt_count: 0,
            last_attempt_at: None,
            delivered_at: None,
            claimed_at: None,
            last_error_code: None,
            last_error_message: None,
            created_at: "2026-04-16T20:06:33Z".into(),
            updated_at: "2026-04-16T20:06:33Z".into(),
        }],
    })
    .expect("list notification dispatches response should serialize");
    assert_eq!(
        list_dispatches_response,
        json!({
            "dispatches": [
                {
                    "id": 1,
                    "projectId": "project-1",
                    "actionId": "session:session-1:review_worktree",
                    "routeId": "route-discord",
                    "correlationKey": "nfy:11111111111111111111111111111111",
                    "status": "pending",
                    "attemptCount": 0,
                    "lastAttemptAt": null,
                    "deliveredAt": null,
                    "claimedAt": null,
                    "lastErrorCode": null,
                    "lastErrorMessage": null,
                    "createdAt": "2026-04-16T20:06:33Z",
                    "updatedAt": "2026-04-16T20:06:33Z"
                }
            ]
        })
    );

    let record_dispatch_outcome_response =
        serde_json::to_value(RecordNotificationDispatchOutcomeResponseDto {
            dispatch: NotificationDispatchDto {
                id: 1,
                project_id: "project-1".into(),
                action_id: "session:session-1:review_worktree".into(),
                route_id: "route-discord".into(),
                correlation_key: "nfy:11111111111111111111111111111111".into(),
                status: NotificationDispatchStatusDto::Sent,
                attempt_count: 1,
                last_attempt_at: Some("2026-04-16T20:06:34Z".into()),
                delivered_at: Some("2026-04-16T20:06:34Z".into()),
                claimed_at: None,
                last_error_code: None,
                last_error_message: None,
                created_at: "2026-04-16T20:06:33Z".into(),
                updated_at: "2026-04-16T20:06:34Z".into(),
            },
        })
        .expect("record notification dispatch outcome response should serialize");
    assert_eq!(
        record_dispatch_outcome_response,
        json!({
            "dispatch": {
                "id": 1,
                "projectId": "project-1",
                "actionId": "session:session-1:review_worktree",
                "routeId": "route-discord",
                "correlationKey": "nfy:11111111111111111111111111111111",
                "status": "sent",
                "attemptCount": 1,
                "lastAttemptAt": "2026-04-16T20:06:34Z",
                "deliveredAt": "2026-04-16T20:06:34Z",
                "claimedAt": null,
                "lastErrorCode": null,
                "lastErrorMessage": null,
                "createdAt": "2026-04-16T20:06:33Z",
                "updatedAt": "2026-04-16T20:06:34Z"
            }
        })
    );

    let submit_notification_reply_response =
        serde_json::to_value(SubmitNotificationReplyResponseDto {
            claim: NotificationReplyClaimDto {
                id: 9,
                project_id: "project-1".into(),
                action_id: "session:session-1:review_worktree".into(),
                route_id: "route-discord".into(),
                correlation_key: "nfy:11111111111111111111111111111111".into(),
                responder_id: Some("operator-a".into()),
                status: NotificationReplyClaimStatusDto::Accepted,
                rejection_code: None,
                rejection_message: None,
                created_at: "2026-04-16T20:06:35Z".into(),
            },
            dispatch: NotificationDispatchDto {
                id: 1,
                project_id: "project-1".into(),
                action_id: "session:session-1:review_worktree".into(),
                route_id: "route-discord".into(),
                correlation_key: "nfy:11111111111111111111111111111111".into(),
                status: NotificationDispatchStatusDto::Claimed,
                attempt_count: 1,
                last_attempt_at: Some("2026-04-16T20:06:34Z".into()),
                delivered_at: Some("2026-04-16T20:06:34Z".into()),
                claimed_at: Some("2026-04-16T20:06:35Z".into()),
                last_error_code: None,
                last_error_message: None,
                created_at: "2026-04-16T20:06:33Z".into(),
                updated_at: "2026-04-16T20:06:35Z".into(),
            },
            resolve_result: ResolveOperatorActionResponseDto {
                approval_request: sample_snapshot().approval_requests[0].clone(),
                verification_record: VerificationRecordDto {
                    id: 8,
                    source_action_id: Some("session:session-1:review_worktree".into()),
                    status: VerificationRecordStatus::Passed,
                    summary: "Approved operator action: Repository has local changes.".into(),
                    detail: Some("Looks good".into()),
                    recorded_at: "2026-04-16T20:06:35Z".into(),
                },
            },
            resume_result: Some(ResumeOperatorRunResponseDto {
                approval_request: sample_snapshot().approval_requests[0].clone(),
                resume_entry: sample_snapshot().resume_history[0].clone(),
                automatic_dispatch: Some(sample_automatic_dispatch_outcome()),
            }),
        })
        .expect("submit notification reply response should serialize");
    assert_eq!(
        submit_notification_reply_response["claim"]["status"],
        json!("accepted")
    );
    assert_eq!(
        submit_notification_reply_response["dispatch"]["status"],
        json!("claimed")
    );

    assert_eq!(
        serde_json::to_value(ResumeHistoryStatus::Failed)
            .expect("failed resume-history status should serialize"),
        json!("failed")
    );

    let subscribe_request = serde_json::to_value(SubscribeRuntimeStreamRequestDto {
        project_id: "project-1".into(),
        channel: Some("__CHANNEL__:77".into()),
        item_kinds: vec![
            "transcript".into(),
            "tool".into(),
            "activity".into(),
            "failure".into(),
        ],
    })
    .expect("stream subscribe request should serialize");
    assert_eq!(
        subscribe_request,
        json!({
            "projectId": "project-1",
            "channel": "__CHANNEL__:77",
            "itemKinds": ["transcript", "tool", "activity", "failure"]
        })
    );

    let subscribe_response = serde_json::to_value(SubscribeRuntimeStreamResponseDto {
        project_id: "project-1".into(),
        runtime_kind: "openai_codex".into(),
        run_id: "run-1".into(),
        session_id: "session-1".into(),
        flow_id: None,
        subscribed_item_kinds: vec![
            RuntimeStreamItemKind::Transcript,
            RuntimeStreamItemKind::Tool,
            RuntimeStreamItemKind::Activity,
            RuntimeStreamItemKind::Failure,
        ],
    })
    .expect("stream subscribe response should serialize");
    assert_eq!(
        subscribe_response,
        json!({
            "projectId": "project-1",
            "runtimeKind": "openai_codex",
            "runId": "run-1",
            "sessionId": "session-1",
            "flowId": null,
            "subscribedItemKinds": ["transcript", "tool", "activity", "failure"]
        })
    );

    let stream_item = serde_json::to_value(RuntimeStreamItemDto {
        kind: RuntimeStreamItemKind::Activity,
        run_id: "run-1".into(),
        sequence: 7,
        session_id: Some("session-1".into()),
        flow_id: Some("flow-1".into()),
        text: None,
        tool_call_id: None,
        tool_name: None,
        tool_state: None,
        action_id: None,
        boundary_id: None,
        action_type: None,
        title: Some("Planning".into()),
        detail: Some("Replay buffer ready".into()),
        code: Some("phase_progress".into()),
        message: None,
        retryable: None,
        created_at: "2026-04-13T14:11:59Z".into(),
    })
    .expect("runtime stream item should serialize");
    assert_eq!(
        stream_item,
        json!({
            "kind": "activity",
            "runId": "run-1",
            "sequence": 7,
            "sessionId": "session-1",
            "flowId": "flow-1",
            "text": null,
            "toolCallId": null,
            "toolName": null,
            "toolState": null,
            "actionId": null,
            "boundaryId": null,
            "actionType": null,
            "title": "Planning",
            "detail": "Replay buffer ready",
            "code": "phase_progress",
            "message": null,
            "retryable": null,
            "createdAt": "2026-04-13T14:11:59Z"
        })
    );

    let upsert_graph_request = serde_json::to_value(UpsertWorkflowGraphRequestDto {
        project_id: "project-1".into(),
        nodes: vec![WorkflowGraphNodeDto {
            node_id: "plan".into(),
            phase_id: 1,
            sort_order: 1,
            name: "Plan".into(),
            description: "Plan workflow".into(),
            status: cadence_desktop_lib::commands::PhaseStatus::Active,
            current_step: Some(cadence_desktop_lib::commands::PhaseStep::Plan),
            task_count: 2,
            completed_tasks: 1,
            summary: Some("In progress".into()),
        }],
        edges: vec![WorkflowGraphEdgeDto {
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_requirement: Some("execution_gate".into()),
        }],
        gates: vec![WorkflowGraphGateRequestDto {
            node_id: "execute".into(),
            gate_key: "execution_gate".into(),
            gate_state: "pending".into(),
            action_type: Some("approve_execution".into()),
            title: Some("Approve execution".into()),
            detail: Some("Operator approval required.".into()),
            decision_context: None,
        }],
    })
    .expect("workflow graph upsert request should serialize");
    assert_eq!(
        upsert_graph_request,
        json!({
            "projectId": "project-1",
            "nodes": [
                {
                    "nodeId": "plan",
                    "phaseId": 1,
                    "sortOrder": 1,
                    "name": "Plan",
                    "description": "Plan workflow",
                    "status": "active",
                    "currentStep": "plan",
                    "taskCount": 2,
                    "completedTasks": 1,
                    "summary": "In progress"
                }
            ],
            "edges": [
                {
                    "fromNodeId": "plan",
                    "toNodeId": "execute",
                    "transitionKind": "advance",
                    "gateRequirement": "execution_gate"
                }
            ],
            "gates": [
                {
                    "nodeId": "execute",
                    "gateKey": "execution_gate",
                    "gateState": "pending",
                    "actionType": "approve_execution",
                    "title": "Approve execution",
                    "detail": "Operator approval required.",
                    "decisionContext": null
                }
            ]
        })
    );

    let upsert_graph_response = serde_json::to_value(UpsertWorkflowGraphResponseDto {
        nodes: vec![WorkflowGraphNodeDto {
            node_id: "plan".into(),
            phase_id: 1,
            sort_order: 1,
            name: "Plan".into(),
            description: "Plan workflow".into(),
            status: cadence_desktop_lib::commands::PhaseStatus::Active,
            current_step: Some(cadence_desktop_lib::commands::PhaseStep::Plan),
            task_count: 2,
            completed_tasks: 1,
            summary: Some("In progress".into()),
        }],
        edges: vec![WorkflowGraphEdgeDto {
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_requirement: Some("execution_gate".into()),
        }],
        gates: vec![WorkflowGateMetadataDto {
            node_id: "execute".into(),
            gate_key: "execution_gate".into(),
            gate_state: WorkflowGateStateDto::Pending,
            action_type: Some("approve_execution".into()),
            title: Some("Approve execution".into()),
            detail: Some("Operator approval required.".into()),
            decision_context: None,
        }],
        phases: vec![cadence_desktop_lib::commands::PhaseSummaryDto {
            id: 1,
            name: "Plan".into(),
            description: "Plan workflow".into(),
            status: cadence_desktop_lib::commands::PhaseStatus::Active,
            current_step: Some(cadence_desktop_lib::commands::PhaseStep::Plan),
            task_count: 2,
            completed_tasks: 1,
            summary: Some("In progress".into()),
        }],
    })
    .expect("workflow graph upsert response should serialize");
    assert_eq!(
        upsert_graph_response,
        json!({
            "nodes": [
                {
                    "nodeId": "plan",
                    "phaseId": 1,
                    "sortOrder": 1,
                    "name": "Plan",
                    "description": "Plan workflow",
                    "status": "active",
                    "currentStep": "plan",
                    "taskCount": 2,
                    "completedTasks": 1,
                    "summary": "In progress"
                }
            ],
            "edges": [
                {
                    "fromNodeId": "plan",
                    "toNodeId": "execute",
                    "transitionKind": "advance",
                    "gateRequirement": "execution_gate"
                }
            ],
            "gates": [
                {
                    "nodeId": "execute",
                    "gateKey": "execution_gate",
                    "gateState": "pending",
                    "actionType": "approve_execution",
                    "title": "Approve execution",
                    "detail": "Operator approval required.",
                    "decisionContext": null
                }
            ],
            "phases": [
                {
                    "id": 1,
                    "name": "Plan",
                    "description": "Plan workflow",
                    "status": "active",
                    "currentStep": "plan",
                    "taskCount": 2,
                    "completedTasks": 1,
                    "summary": "In progress"
                }
            ]
        })
    );

    let apply_transition_request = serde_json::to_value(ApplyWorkflowTransitionRequestDto {
        project_id: "project-1".into(),
        transition_id: "txn-002".into(),
        causal_transition_id: Some("txn-001".into()),
        from_node_id: "plan".into(),
        to_node_id: "execute".into(),
        transition_kind: "advance".into(),
        gate_decision: "approved".into(),
        gate_decision_context: Some("operator-approved".into()),
        gate_updates: vec![WorkflowTransitionGateUpdateRequestDto {
            gate_key: "execution_gate".into(),
            gate_state: "satisfied".into(),
            decision_context: Some("approved by operator".into()),
        }],
        occurred_at: "2026-04-15T18:01:00Z".into(),
    })
    .expect("apply workflow transition request should serialize");
    assert_eq!(
        apply_transition_request,
        json!({
            "projectId": "project-1",
            "transitionId": "txn-002",
            "causalTransitionId": "txn-001",
            "fromNodeId": "plan",
            "toNodeId": "execute",
            "transitionKind": "advance",
            "gateDecision": "approved",
            "gateDecisionContext": "operator-approved",
            "gateUpdates": [
                {
                    "gateKey": "execution_gate",
                    "gateState": "satisfied",
                    "decisionContext": "approved by operator"
                }
            ],
            "occurredAt": "2026-04-15T18:01:00Z"
        })
    );

    let apply_transition_response = serde_json::to_value(ApplyWorkflowTransitionResponseDto {
        transition_event: sample_transition_event(),
        automatic_dispatch: sample_automatic_dispatch_outcome(),
        phases: vec![cadence_desktop_lib::commands::PhaseSummaryDto {
            id: 2,
            name: "Execute".into(),
            description: "Run work".into(),
            status: cadence_desktop_lib::commands::PhaseStatus::Active,
            current_step: Some(cadence_desktop_lib::commands::PhaseStep::Execute),
            task_count: 4,
            completed_tasks: 1,
            summary: None,
        }],
    })
    .expect("apply workflow transition response should serialize");
    assert_eq!(
        apply_transition_response,
        json!({
            "transitionEvent": {
                "id": 12,
                "transitionId": "txn-002",
                "causalTransitionId": "txn-001",
                "fromNodeId": "plan",
                "toNodeId": "execute",
                "transitionKind": "advance",
                "gateDecision": "approved",
                "gateDecisionContext": "operator-approved",
                "createdAt": "2026-04-15T18:01:00Z"
            },
            "automaticDispatch": {
                "status": "applied",
                "transitionEvent": {
                    "id": 12,
                    "transitionId": "txn-002",
                    "causalTransitionId": "txn-001",
                    "fromNodeId": "plan",
                    "toNodeId": "execute",
                    "transitionKind": "advance",
                    "gateDecision": "approved",
                    "gateDecisionContext": "operator-approved",
                    "createdAt": "2026-04-15T18:01:00Z"
                },
                "handoffPackage": {
                    "status": "persisted",
                    "package": {
                        "id": 21,
                        "projectId": "project-1",
                        "handoffTransitionId": "auto:txn-002:workflow-discussion:workflow-research",
                        "causalTransitionId": "txn-001",
                        "fromNodeId": "workflow-discussion",
                        "toNodeId": "workflow-research",
                        "transitionKind": "advance",
                        "packagePayload": "{\"schemaVersion\":1,\"triggerTransition\":{\"transitionId\":\"auto:txn-002:workflow-discussion:workflow-research\"}}",
                        "packageHash": "a18fc57e3d2b8f4ef67f5b50f37ba7d85f49a1be987f17fa9dc0ad5a64ff8322",
                        "createdAt": "2026-04-15T18:01:01Z"
                    },
                    "code": null,
                    "message": null
                },
                "code": null,
                "message": null
            },
            "phases": [
                {
                    "id": 2,
                    "name": "Execute",
                    "description": "Run work",
                    "status": "active",
                    "currentStep": "execute",
                    "taskCount": 4,
                    "completedTasks": 1,
                    "summary": null
                }
            ]
        })
    );

    let skipped_dispatch = serde_json::to_value(sample_skipped_automatic_dispatch_outcome())
        .expect("skipped automatic-dispatch outcome should serialize");
    assert_eq!(skipped_dispatch["status"], json!("skipped"));
    assert_eq!(skipped_dispatch["transitionEvent"], Value::Null);
    assert_eq!(skipped_dispatch["handoffPackage"], Value::Null);
    assert_eq!(
        skipped_dispatch["code"],
        json!("workflow_transition_gate_unmet")
    );
    assert!(
        skipped_dispatch["message"]
            .as_str()
            .expect("skipped dispatch message should serialize")
            .contains("Persisted pending operator approval"),
        "skipped diagnostics should preserve persisted-approval context"
    );

    assert_eq!(
        RuntimeStreamItemDto::allowed_kind_names(),
        &[
            "transcript",
            "tool",
            "activity",
            "action_required",
            "complete",
            "failure"
        ]
    );

    assert_eq!(PROJECT_UPDATED_EVENT, "project:updated");
    assert_eq!(REPOSITORY_STATUS_CHANGED_EVENT, "repository:status_changed");
    assert_eq!(RUNTIME_UPDATED_EVENT, "runtime:updated");
    assert_eq!(RUNTIME_RUN_UPDATED_EVENT, "runtime_run:updated");
    assert_eq!(START_OPENAI_CODEX_AUTH_COMMAND, "start_openai_login");
    assert_eq!(COMPLETE_OPENAI_CODEX_AUTH_COMMAND, "submit_openai_callback");
    assert_eq!(CANCEL_OPENAI_CODEX_AUTH_COMMAND, "cancel_openai_codex_auth");
    assert_eq!(GET_RUNTIME_AUTH_STATUS_COMMAND, "get_runtime_session");
    assert_eq!(GET_RUNTIME_RUN_COMMAND, "get_runtime_run");
    assert_eq!(
        cadence_desktop_lib::commands::GET_RUNTIME_SETTINGS_COMMAND,
        "get_runtime_settings"
    );
    assert_eq!(REFRESH_OPENAI_CODEX_AUTH_COMMAND, "start_runtime_session");
    assert_eq!(START_RUNTIME_RUN_COMMAND, "start_runtime_run");
    assert_eq!(
        cadence_desktop_lib::commands::UPSERT_RUNTIME_SETTINGS_COMMAND,
        "upsert_runtime_settings"
    );
    assert_eq!(STOP_RUNTIME_RUN_COMMAND, "stop_runtime_run");
    assert_eq!(SUBSCRIBE_RUNTIME_STREAM_COMMAND, "subscribe_runtime_stream");
    assert_eq!(RESOLVE_OPERATOR_ACTION_COMMAND, "resolve_operator_action");
    assert_eq!(RESUME_OPERATOR_RUN_COMMAND, "resume_operator_run");
    assert_eq!(LIST_NOTIFICATION_ROUTES_COMMAND, "list_notification_routes");
    assert_eq!(
        LIST_NOTIFICATION_DISPATCHES_COMMAND,
        "list_notification_dispatches"
    );
    assert_eq!(
        UPSERT_NOTIFICATION_ROUTE_COMMAND,
        "upsert_notification_route"
    );
    assert_eq!(
        UPSERT_NOTIFICATION_ROUTE_CREDENTIALS_COMMAND,
        "upsert_notification_route_credentials"
    );
    assert_eq!(
        RECORD_NOTIFICATION_DISPATCH_OUTCOME_COMMAND,
        "record_notification_dispatch_outcome"
    );
    assert_eq!(
        SUBMIT_NOTIFICATION_REPLY_COMMAND,
        "submit_notification_reply"
    );
    assert_eq!(
        SYNC_NOTIFICATION_ADAPTERS_COMMAND,
        "sync_notification_adapters"
    );
    assert_eq!(UPSERT_WORKFLOW_GRAPH_COMMAND, "upsert_workflow_graph");
    assert_eq!(
        APPLY_WORKFLOW_TRANSITION_COMMAND,
        "apply_workflow_transition"
    );
}

#[test]
fn malformed_inputs_fail_fast_before_runtime_logic() {
    assert!(serde_json::from_value::<ImportRepositoryRequestDto>(json!({})).is_err());
    assert!(
        serde_json::from_value::<ProjectIdRequestDto>(json!({ "projectID": "project-1" })).is_err()
    );
    assert!(
        serde_json::from_value::<GetRuntimeRunRequestDto>(json!({
            "projectId": "project-1",
            "unexpected": true
        }))
        .is_err(),
        "get runtime run request should reject unknown fields"
    );
    assert!(
        serde_json::from_value::<StopRuntimeRunRequestDto>(json!({
            "projectId": "project-1",
            "runID": "run-1"
        }))
        .is_err(),
        "stop runtime run request should require camelCase runId"
    );
    assert!(serde_json::from_value::<RepositoryDiffRequestDto>(json!({
        "projectId": "project-1",
        "scope": "UNSTAGED"
    }))
    .is_err());
    assert!(
        serde_json::from_value::<ResolveOperatorActionRequestDto>(json!({
            "projectId": "project-1",
            "actionId": "session:session-1:review_worktree",
            "decision": "approve",
            "decisionNote": "legacy-field-should-be-rejected"
        }))
        .is_err(),
        "resolve request should reject legacy decisionNote payloads"
    );
    assert!(
        serde_json::from_value::<ResumeOperatorRunRequestDto>(json!({
            "projectId": "project-1",
            "actionId": "session:session-1:review_worktree",
            "decisionNote": "unexpected"
        }))
        .is_err(),
        "resume request should reject unknown fields"
    );
    assert!(
        serde_json::from_value::<ListNotificationRoutesRequestDto>(json!({
            "projectId": "project-1",
            "unexpected": true
        }))
        .is_err(),
        "list notification routes request should reject unknown fields"
    );
    assert!(
        serde_json::from_value::<UpsertNotificationRouteRequestDto>(json!({
            "projectId": "project-1",
            "routeId": "route-discord",
            "routeKind": "discord",
            "routeTarget": "123456789012345678",
            "enabled": true,
            "metadataJson": "{\"channel\":\"ops\"}",
            "updatedAt": "2026-04-16T20:06:33Z",
            "unexpected": true
        }))
        .is_err(),
        "upsert notification route request should reject unknown fields"
    );
    assert!(
        serde_json::from_value::<UpsertNotificationRouteRequestDto>(json!({
            "projectId": "project-1",
            "routeId": "route-discord",
            "routeKind": 7,
            "routeTarget": "123456789012345678",
            "enabled": true,
            "metadataJson": "{\"channel\":\"ops\"}",
            "updatedAt": "2026-04-16T20:06:33Z"
        }))
        .is_err(),
        "upsert notification route request should reject non-string routeKind payloads"
    );
    assert!(
        serde_json::from_value::<UpsertNotificationRouteCredentialsRequestDto>(json!({
            "projectId": "project-1",
            "routeId": "route-discord",
            "routeKind": "discord",
            "credentials": {
                "botToken": "discord-bot-token",
                "webhookUrl": "https://discord.com/api/webhooks/1/2",
                "unexpected": true
            },
            "updatedAt": "2026-04-16T20:06:33Z"
        }))
        .is_err(),
        "upsert notification route credentials request should reject unknown credential fields"
    );
    assert!(
        serde_json::from_value::<UpsertNotificationRouteCredentialsRequestDto>(json!({
            "projectId": "project-1",
            "routeId": "route-discord",
            "routeKind": "discord",
            "credentials": {
                "botToken": 7,
                "webhookUrl": "https://discord.com/api/webhooks/1/2"
            },
            "updatedAt": "2026-04-16T20:06:33Z"
        }))
        .is_err(),
        "upsert notification route credentials request should reject non-string credential payloads"
    );
    assert!(
        serde_json::from_value::<UpsertNotificationRouteCredentialsResponseDto>(json!({
            "projectId": "project-1",
            "routeId": "route-discord",
            "routeKind": "email",
            "credentialScope": "app_local",
            "hasBotToken": true,
            "hasChatId": false,
            "hasWebhookUrl": true,
            "updatedAt": "2026-04-16T20:06:33Z"
        }))
        .is_err(),
        "upsert notification route credentials response should reject unsupported route-kind enums"
    );
    assert!(
        serde_json::from_value::<ListNotificationDispatchesRequestDto>(json!({
            "projectId": "project-1",
            "actionId": "session:session-1:review_worktree",
            "unexpected": true
        }))
        .is_err(),
        "list notification dispatches request should reject unknown fields"
    );
    assert!(
        serde_json::from_value::<SyncNotificationAdaptersRequestDto>(json!({
            "projectId": "project-1",
            "unexpected": true
        }))
        .is_err(),
        "sync notification adapters request should reject unknown fields"
    );
    assert!(
        serde_json::from_value::<SyncNotificationAdaptersResponseDto>(json!({
            "projectId": "project-1",
            "dispatch": {
                "projectId": "project-1",
                "pendingCount": 1,
                "attemptedCount": 1,
                "sentCount": 0,
                "failedCount": 1,
                "attemptLimit": 64,
                "attemptsTruncated": false,
                "attempts": [
                    {
                        "dispatchId": 1,
                        "actionId": "session:session-1:review_worktree",
                        "routeId": "route-discord",
                        "routeKind": "discord",
                        "outcomeStatus": "queued",
                        "diagnosticCode": "notification_adapter_dispatch_failed",
                        "diagnosticMessage": "failed",
                        "durableErrorCode": "notification_adapter_transport_failed",
                        "durableErrorMessage": "transport error"
                    }
                ],
                "errorCodeCounts": []
            },
            "replies": {
                "projectId": "project-1",
                "routeCount": 1,
                "polledRouteCount": 1,
                "messageCount": 0,
                "acceptedCount": 0,
                "rejectedCount": 1,
                "attemptLimit": 256,
                "attemptsTruncated": false,
                "attempts": [],
                "errorCodeCounts": []
            },
            "syncedAt": "2026-04-16T20:06:35Z"
        }))
        .is_err(),
        "sync notification adapters response should reject unsupported dispatch status enums"
    );
    assert!(
        serde_json::from_value::<RecordNotificationDispatchOutcomeRequestDto>(json!({
            "projectId": "project-1",
            "actionId": "session:session-1:review_worktree",
            "routeId": "route-discord",
            "status": "pending",
            "attemptedAt": "2026-04-16T20:06:33Z",
            "errorCode": null,
            "errorMessage": null
        }))
        .is_err(),
        "record notification dispatch outcome request should reject unsupported status enums"
    );
    assert!(
        serde_json::from_value::<SubmitNotificationReplyRequestDto>(json!({
            "projectId": "project-1",
            "actionId": "session:session-1:review_worktree",
            "routeId": "route-discord",
            "correlationKey": "nfy:11111111111111111111111111111111",
            "responderId": "operator-a",
            "replyText": "Looks good",
            "decision": "approve",
            "receivedAt": "2026-04-16T20:06:34Z",
            "unexpected": true
        }))
        .is_err(),
        "submit notification reply request should reject unknown fields"
    );
    assert!(
        serde_json::from_value::<ListNotificationRoutesResponseDto>(json!({
            "routes": [
                {
                    "projectId": "project-1",
                    "routeId": "route-discord",
                    "routeKind": "email",
                    "routeTarget": "123456789012345678",
                    "enabled": true,
                    "metadataJson": "{\"channel\":\"ops\"}",
                    "createdAt": "2026-04-16T20:05:30Z",
                    "updatedAt": "2026-04-16T20:06:33Z"
                }
            ]
        }))
        .is_err(),
        "list notification routes response should reject unsupported route kind enums"
    );

    assert!(
        serde_json::from_value::<UpsertWorkflowGraphRequestDto>(json!({
            "projectId": "project-1",
            "nodes": [
                {
                    "nodeId": "plan",
                    "phaseId": 1,
                    "sortOrder": 1,
                    "name": "Plan",
                    "description": "Plan phase",
                    "status": "active",
                    "currentStep": "plan",
                    "taskCount": 1,
                    "completedTasks": 0,
                    "summary": null,
                    "extra": true
                }
            ],
            "edges": [],
            "gates": []
        }))
        .is_err(),
        "upsert request should reject unknown node fields"
    );

    assert!(
        serde_json::from_value::<ApplyWorkflowTransitionRequestDto>(json!({
            "projectId": "project-1",
            "transitionId": "txn-1",
            "causalTransitionId": null,
            "fromNodeId": "plan",
            "toNodeId": "execute",
            "transitionKind": "advance",
            "gateDecision": "approved",
            "gateDecisionContext": null,
            "gateUpdates": [
                {
                    "gateKey": "execution_gate",
                    "gateState": "satisfied",
                    "decisionContext": null,
                    "unexpected": true
                }
            ],
            "occurredAt": "2026-04-13T20:01:00Z"
        }))
        .is_err(),
        "transition request should reject unknown gate-update fields"
    );

    assert!(
        serde_json::from_value::<ApplyWorkflowTransitionRequestDto>(json!({
            "projectId": "project-1",
            "transitionId": "txn-1",
            "causalTransitionId": null,
            "fromNodeId": "plan",
            "toNodeId": "execute",
            "transitionKind": "advance",
            "gateDecision": 7,
            "gateDecisionContext": null,
            "gateUpdates": [],
            "occurredAt": "2026-04-13T20:01:00Z"
        }))
        .is_err(),
        "transition request should reject non-string gateDecision payloads"
    );

    let mut snapshot_with_unknown_lifecycle_stage =
        serde_json::to_value(sample_snapshot()).expect("serialize snapshot fixture");
    snapshot_with_unknown_lifecycle_stage["lifecycle"]["stages"][0]["stage"] = json!("discovery");
    assert!(
        serde_json::from_value::<ProjectSnapshotResponseDto>(snapshot_with_unknown_lifecycle_stage)
            .is_err(),
        "snapshot payload should reject unknown lifecycle stage enums"
    );

    let mut snapshot_with_unknown_lifecycle_field =
        serde_json::to_value(sample_snapshot()).expect("serialize snapshot fixture");
    snapshot_with_unknown_lifecycle_field["lifecycle"]["stages"][0]["unexpected"] = json!(true);
    assert!(
        serde_json::from_value::<ProjectSnapshotResponseDto>(snapshot_with_unknown_lifecycle_field)
            .is_err(),
        "snapshot payload should reject unknown lifecycle stage fields"
    );

    let mut snapshot_with_unknown_handoff_field =
        serde_json::to_value(sample_snapshot()).expect("serialize snapshot fixture");
    snapshot_with_unknown_handoff_field["handoffPackages"][0]["unexpected"] = json!(true);
    assert!(
        serde_json::from_value::<ProjectSnapshotResponseDto>(snapshot_with_unknown_handoff_field)
            .is_err(),
        "snapshot payload should reject unknown handoff package fields"
    );

    let mut snapshot_with_malformed_handoff_hash =
        serde_json::to_value(sample_snapshot()).expect("serialize snapshot fixture");
    snapshot_with_malformed_handoff_hash["handoffPackages"][0]["packageHash"] = json!(7);
    assert!(
        serde_json::from_value::<ProjectSnapshotResponseDto>(snapshot_with_malformed_handoff_hash)
            .is_err(),
        "snapshot payload should reject malformed handoff package hashes"
    );

    let mut transition_response_with_unknown_dispatch_status =
        serde_json::to_value(ApplyWorkflowTransitionResponseDto {
            transition_event: sample_transition_event(),
            automatic_dispatch: sample_automatic_dispatch_outcome(),
            phases: Vec::new(),
        })
        .expect("serialize apply transition response fixture");
    transition_response_with_unknown_dispatch_status["automaticDispatch"]["status"] =
        json!("continued");
    assert!(
        serde_json::from_value::<ApplyWorkflowTransitionResponseDto>(
            transition_response_with_unknown_dispatch_status,
        )
        .is_err(),
        "transition response should reject unknown automatic-dispatch status values"
    );

    let mut transition_response_with_malformed_skipped_dispatch =
        serde_json::to_value(ApplyWorkflowTransitionResponseDto {
            transition_event: sample_transition_event(),
            automatic_dispatch: sample_skipped_automatic_dispatch_outcome(),
            phases: Vec::new(),
        })
        .expect("serialize skipped apply transition response fixture");
    transition_response_with_malformed_skipped_dispatch["automaticDispatch"]["code"] = json!(7);
    assert!(
        serde_json::from_value::<ApplyWorkflowTransitionResponseDto>(
            transition_response_with_malformed_skipped_dispatch,
        )
        .is_err(),
        "transition response should reject malformed skipped automatic-dispatch diagnostics"
    );

    let mut resume_response_with_malformed_handoff_package =
        serde_json::to_value(ResumeOperatorRunResponseDto {
            approval_request: sample_snapshot()
                .approval_requests
                .into_iter()
                .next()
                .expect("sample approval exists"),
            resume_entry: sample_snapshot()
                .resume_history
                .into_iter()
                .next()
                .expect("sample resume entry exists"),
            automatic_dispatch: Some(sample_automatic_dispatch_outcome()),
        })
        .expect("serialize resume response fixture");
    resume_response_with_malformed_handoff_package["automaticDispatch"]["handoffPackage"]
        ["package"]["handoffTransitionId"] = json!(null);
    assert!(
        serde_json::from_value::<ResumeOperatorRunResponseDto>(
            resume_response_with_malformed_handoff_package,
        )
        .is_err(),
        "resume response should reject malformed automatic-dispatch handoff package payloads"
    );

    let malformed_runtime_run = json!({
        "projectId": "project-1",
        "runId": "run-1",
        "runtimeKind": "openai_codex",
        "supervisorKind": "detached_pty",
        "status": "awaiting_operator",
        "transport": {
            "kind": "tcp",
            "endpoint": "127.0.0.1:45123",
            "liveness": "reachable"
        },
        "startedAt": "2026-04-15T23:10:00Z",
        "lastHeartbeatAt": "2026-04-15T23:10:01Z",
        "lastCheckpointSequence": 2,
        "lastCheckpointAt": "2026-04-15T23:10:02Z",
        "stoppedAt": null,
        "lastErrorCode": null,
        "lastError": null,
        "updatedAt": "2026-04-15T23:10:02Z",
        "checkpoints": []
    });
    assert!(
        serde_json::from_value::<RuntimeRunDto>(malformed_runtime_run).is_err(),
        "runtime run payload should reject unknown status enums"
    );

    let malformed_runtime_run_event = json!({
        "projectId": "project-1",
        "run": {
            "projectId": "project-1",
            "runId": "run-1",
            "runtimeKind": "openai_codex",
            "supervisorKind": "detached_pty",
            "status": "running",
            "transport": {
                "kind": "tcp",
                "endpoint": "127.0.0.1:45123",
                "liveness": "reachable"
            },
            "startedAt": "2026-04-15T23:10:00Z",
            "lastHeartbeatAt": "2026-04-15T23:10:01Z",
            "lastCheckpointSequence": 2,
            "lastCheckpointAt": "2026-04-15T23:10:02Z",
            "stoppedAt": null,
            "lastErrorCode": null,
            "lastError": null,
            "updatedAt": "2026-04-15T23:10:02Z",
            "checkpoints": [],
            "unexpected": true
        }
    });
    assert!(
        serde_json::from_value::<RuntimeRunUpdatedPayloadDto>(malformed_runtime_run_event).is_err(),
        "runtime run updated payload should reject malformed nested run payloads"
    );
}
