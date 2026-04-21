pub(crate) use cadence_desktop_lib::{
    commands::{
        ApplyWorkflowTransitionRequestDto, ApplyWorkflowTransitionResponseDto,
        AutonomousArtifactPayloadDto, AutonomousCommandResultDto, AutonomousLifecycleReasonDto,
        AutonomousRunDto, AutonomousRunRecoveryStateDto, AutonomousRunStateDto,
        AutonomousRunStatusDto, AutonomousSkillCacheStatusDto, AutonomousSkillLifecycleCacheDto,
        AutonomousSkillLifecycleDiagnosticDto, AutonomousSkillLifecyclePayloadDto,
        AutonomousSkillLifecycleResultDto, AutonomousSkillLifecycleSourceDto,
        AutonomousSkillLifecycleStageDto, AutonomousToolCallStateDto,
        AutonomousToolResultPayloadDto, AutonomousUnitArtifactDto, AutonomousUnitArtifactStatusDto,
        AutonomousUnitAttemptDto, AutonomousUnitDto, AutonomousUnitHistoryEntryDto,
        AutonomousUnitKindDto, AutonomousUnitStatusDto, AutonomousWorkflowLinkageDto,
        BranchSummaryDto, CancelAutonomousRunRequestDto, ChangeKind, CommandError,
        CommandErrorClass, CommandToolResultSummaryDto, FileToolResultSummaryDto,
        GetAutonomousRunRequestDto, GetRuntimeRunRequestDto, GitToolResultScopeDto,
        GitToolResultSummaryDto, ImportRepositoryRequestDto, ListNotificationDispatchesRequestDto,
        ListNotificationDispatchesResponseDto, ListNotificationRoutesRequestDto,
        ListNotificationRoutesResponseDto, ListProjectsResponseDto, NotificationDispatchDto,
        NotificationDispatchOutcomeStatusDto, NotificationDispatchStatusDto,
        NotificationReplyClaimDto, NotificationReplyClaimStatusDto, NotificationRouteDto,
        NotificationRouteKindDto, OperatorApprovalDto, OperatorApprovalStatus,
        PlanningLifecycleProjectionDto, PlanningLifecycleStageDto, PlanningLifecycleStageKindDto,
        ProjectIdRequestDto, ProjectSnapshotResponseDto, ProjectSummaryDto, ProjectUpdateReason,
        ProjectUpdatedPayloadDto, RecordNotificationDispatchOutcomeRequestDto,
        RecordNotificationDispatchOutcomeResponseDto, RepositoryDiffRequestDto,
        RepositoryDiffResponseDto, RepositoryDiffScope, RepositoryStatusChangedPayloadDto,
        RepositoryStatusEntryDto, RepositoryStatusResponseDto, RepositorySummaryDto,
        ResolveOperatorActionRequestDto, ResolveOperatorActionResponseDto, ResumeHistoryEntryDto,
        ResumeHistoryStatus, ResumeOperatorRunRequestDto, ResumeOperatorRunResponseDto,
        RuntimeAuthPhase, RuntimeAuthStatusDto, RuntimeDiagnosticDto, RuntimeRunCheckpointDto,
        RuntimeRunCheckpointKindDto, RuntimeRunDiagnosticDto, RuntimeRunDto, RuntimeRunStatusDto,
        RuntimeRunTransportDto, RuntimeRunTransportLivenessDto, RuntimeRunUpdatedPayloadDto,
        RuntimeStreamItemDto, RuntimeStreamItemKind, RuntimeUpdatedPayloadDto,
        StartAutonomousRunRequestDto, StartOpenAiLoginRequestDto, StartRuntimeRunRequestDto,
        StopRuntimeRunRequestDto, SubmitNotificationReplyRequestDto,
        SubmitNotificationReplyResponseDto, SubmitOpenAiCallbackRequestDto,
        SubscribeRuntimeStreamRequestDto, SubscribeRuntimeStreamResponseDto,
        SyncNotificationAdaptersRequestDto, SyncNotificationAdaptersResponseDto,
        ToolResultSummaryDto, UpsertNotificationRouteCredentialsRequestDto,
        UpsertNotificationRouteCredentialsResponseDto, UpsertNotificationRouteRequestDto,
        UpsertNotificationRouteResponseDto, UpsertWorkflowGraphRequestDto,
        UpsertWorkflowGraphResponseDto, VerificationRecordDto, VerificationRecordStatus,
        WebToolResultContentKindDto, WebToolResultSummaryDto, WorkflowAutomaticDispatchOutcomeDto,
        WorkflowAutomaticDispatchPackageOutcomeDto, WorkflowAutomaticDispatchPackageStatusDto,
        WorkflowAutomaticDispatchStatusDto, WorkflowGateMetadataDto, WorkflowGateStateDto,
        WorkflowGraphEdgeDto, WorkflowGraphGateRequestDto, WorkflowGraphNodeDto,
        WorkflowHandoffPackageDto, WorkflowTransitionEventDto, WorkflowTransitionGateDecisionDto,
        WorkflowTransitionGateUpdateRequestDto, APPLY_WORKFLOW_TRANSITION_COMMAND,
        CANCEL_AUTONOMOUS_RUN_COMMAND, CANCEL_OPENAI_CODEX_AUTH_COMMAND,
        COMPLETE_OPENAI_CODEX_AUTH_COMMAND, GET_AUTONOMOUS_RUN_COMMAND,
        GET_PROJECT_SNAPSHOT_COMMAND, GET_REPOSITORY_DIFF_COMMAND, GET_REPOSITORY_STATUS_COMMAND,
        LIST_PROJECT_FILES_COMMAND, READ_PROJECT_FILE_COMMAND, WRITE_PROJECT_FILE_COMMAND,
        CREATE_PROJECT_ENTRY_COMMAND, RENAME_PROJECT_ENTRY_COMMAND,
        DELETE_PROJECT_ENTRY_COMMAND, GET_RUNTIME_AUTH_STATUS_COMMAND,
        GET_RUNTIME_RUN_COMMAND, IMPORT_REPOSITORY_COMMAND,
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
pub(crate) use serde_json::{json, Value};
pub(crate) use tempfile::TempDir;

pub(crate) fn build_mock_app() -> (tauri::App<tauri::test::MockRuntime>, TempDir) {
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

pub(crate) fn invoke_request(command: &str, payload: Value) -> tauri::webview::InvokeRequest {
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

pub(crate) fn channel_string() -> String {
    serde_json::to_value(tauri::ipc::Channel::<RuntimeStreamItemDto>::new(|_| Ok(())))
        .expect("channel should serialize")
        .as_str()
        .expect("channel should serialize to string")
        .to_string()
}

pub(crate) fn sample_project() -> ProjectSummaryDto {
    ProjectSummaryDto {
        id: "project-1".into(),
        name: "Cadence".into(),
        description: "Desktop shell".into(),
        milestone: "M001".into(),
        total_phases: 5,
        completed_phases: 1,
        active_phase: 2,
        branch: Some("main".into()),
        runtime: None,
    }
}

pub(crate) fn sample_repository() -> RepositorySummaryDto {
    RepositorySummaryDto {
        id: "repo-1".into(),
        project_id: "project-1".into(),
        root_path: "/tmp/Cadence".into(),
        display_name: "Cadence".into(),
        branch: Some("main".into()),
        head_sha: Some("abc123".into()),
        is_git_repo: true,
    }
}

pub(crate) fn sample_transition_event() -> WorkflowTransitionEventDto {
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

pub(crate) fn sample_handoff_package() -> WorkflowHandoffPackageDto {
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

pub(crate) fn sample_automatic_dispatch_outcome() -> WorkflowAutomaticDispatchOutcomeDto {
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

pub(crate) fn sample_skipped_automatic_dispatch_outcome() -> WorkflowAutomaticDispatchOutcomeDto {
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

pub(crate) fn sample_autonomous_run(duplicate_start_detected: bool) -> AutonomousRunDto {
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

pub(crate) fn sample_autonomous_workflow_linkage() -> AutonomousWorkflowLinkageDto {
    AutonomousWorkflowLinkageDto {
        workflow_node_id: "workflow-research".into(),
        transition_id: "auto:txn-002:workflow-discussion:workflow-research".into(),
        causal_transition_id: Some("txn-001".into()),
        handoff_transition_id: "auto:txn-002:workflow-discussion:workflow-research".into(),
        handoff_package_hash: "a18fc57e3d2b8f4ef67f5b50f37ba7d85f49a1be987f17fa9dc0ad5a64ff8322"
            .into(),
    }
}

pub(crate) fn sample_autonomous_unit() -> AutonomousUnitDto {
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

pub(crate) fn sample_autonomous_attempt() -> AutonomousUnitAttemptDto {
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

pub(crate) fn sample_command_tool_summary() -> ToolResultSummaryDto {
    ToolResultSummaryDto::Command(CommandToolResultSummaryDto {
        exit_code: Some(0),
        timed_out: false,
        stdout_truncated: true,
        stderr_truncated: false,
        stdout_redacted: false,
        stderr_redacted: true,
    })
}

pub(crate) fn sample_file_tool_summary() -> ToolResultSummaryDto {
    ToolResultSummaryDto::File(FileToolResultSummaryDto {
        path: Some("src/lib.rs".into()),
        scope: Some("workspace".into()),
        line_count: Some(120),
        match_count: Some(4),
        truncated: true,
    })
}

pub(crate) fn sample_git_tool_summary() -> ToolResultSummaryDto {
    ToolResultSummaryDto::Git(GitToolResultSummaryDto {
        scope: Some(GitToolResultScopeDto::Worktree),
        changed_files: 3,
        truncated: false,
        base_revision: Some("main~1".into()),
    })
}

pub(crate) fn sample_web_tool_summary() -> ToolResultSummaryDto {
    ToolResultSummaryDto::Web(WebToolResultSummaryDto {
        target: "https://example.com/search?q=Cadence".into(),
        result_count: Some(5),
        final_url: Some("https://example.com/search?q=Cadence".into()),
        content_kind: Some(WebToolResultContentKindDto::Html),
        content_type: Some("text/html".into()),
        truncated: false,
    })
}

pub(crate) fn sample_autonomous_artifact() -> AutonomousUnitArtifactDto {
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
                tool_summary: None,
                action_id: Some("action-1".into()),
                boundary_id: Some("boundary-1".into()),
            },
        )),
        created_at: "2026-04-15T23:10:03Z".into(),
        updated_at: "2026-04-15T23:10:03Z".into(),
    }
}

pub(crate) fn sample_skill_lifecycle_payload() -> AutonomousSkillLifecyclePayloadDto {
    AutonomousSkillLifecyclePayloadDto {
        project_id: "project-1".into(),
        run_id: "run-1".into(),
        unit_id: "run-1:unit:researcher".into(),
        attempt_id: "run-1:unit:researcher:attempt:1".into(),
        artifact_id: "artifact-skill-lifecycle".into(),
        stage: AutonomousSkillLifecycleStageDto::Install,
        result: AutonomousSkillLifecycleResultDto::Failed,
        skill_id: "find-skills".into(),
        source: AutonomousSkillLifecycleSourceDto {
            repo: "vercel-labs/skills".into(),
            path: "skills/find-skills".into(),
            reference: "main".into(),
            tree_hash: "0123456789abcdef0123456789abcdef01234567".into(),
        },
        cache: AutonomousSkillLifecycleCacheDto {
            key: "find-skills-576b45048241".into(),
            status: Some(AutonomousSkillCacheStatusDto::Refreshed),
        },
        diagnostic: Some(AutonomousSkillLifecycleDiagnosticDto {
            code: "autonomous_skill_cache_drift".into(),
            message: "Cadence detected autonomous skill cache drift for `find-skills`.".into(),
            retryable: false,
        }),
    }
}

pub(crate) fn sample_autonomous_history() -> Vec<AutonomousUnitHistoryEntryDto> {
    vec![AutonomousUnitHistoryEntryDto {
        unit: sample_autonomous_unit(),
        latest_attempt: Some(sample_autonomous_attempt()),
        artifacts: vec![sample_autonomous_artifact()],
    }]
}

pub(crate) fn sample_snapshot() -> ProjectSnapshotResponseDto {
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
            detail: "Cadence currently has 3 changed file(s). Review the worktree before trusting subsequent agent actions.".into(),
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
