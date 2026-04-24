use super::support::*;
use cadence_desktop_lib::{
    commands::{
        ProviderModelThinkingEffortDto, RuntimeRunActiveControlSnapshotDto,
        RuntimeRunApprovalModeDto, RuntimeRunControlInputDto, RuntimeRunControlStateDto,
        RuntimeRunPendingControlSnapshotDto,
    },
    runtime::RuntimeSupervisorLaunchContext,
};

pub(crate) fn tool_result_summary_contracts_remain_tagged_and_camel_case_across_nested_payloads() {
    assert_eq!(
        serde_json::to_value(sample_command_tool_summary())
            .expect("command tool summary should serialize"),
        json!({
            "kind": "command",
            "exitCode": 0,
            "timedOut": false,
            "stdoutTruncated": true,
            "stderrTruncated": false,
            "stdoutRedacted": false,
            "stderrRedacted": true
        })
    );

    assert_eq!(
        serde_json::to_value(sample_file_tool_summary())
            .expect("file tool summary should serialize"),
        json!({
            "kind": "file",
            "path": "src/lib.rs",
            "scope": "workspace",
            "lineCount": 120,
            "matchCount": 4,
            "truncated": true
        })
    );

    assert_eq!(
        serde_json::to_value(sample_git_tool_summary()).expect("git tool summary should serialize"),
        json!({
            "kind": "git",
            "scope": "worktree",
            "changedFiles": 3,
            "truncated": false,
            "baseRevision": "main~1"
        })
    );

    assert_eq!(
        serde_json::to_value(sample_web_tool_summary()).expect("web tool summary should serialize"),
        json!({
            "kind": "web",
            "target": "https://example.com/search?q=Cadence",
            "resultCount": 5,
            "finalUrl": "https://example.com/search?q=Cadence",
            "contentKind": "html",
            "contentType": "text/html",
            "truncated": false
        })
    );

    assert_eq!(
        serde_json::to_value(sample_browser_computer_use_tool_summary())
            .expect("browser/computer-use tool summary should serialize"),
        json!({
            "kind": "browser_computer_use",
            "surface": "browser",
            "action": "click",
            "status": "succeeded",
            "target": "button[type=submit]",
            "outcome": "Clicked submit and advanced to the confirmation view."
        })
    );

    let autonomous_tool_payload = serde_json::to_value(AutonomousToolResultPayloadDto {
        project_id: "project-1".into(),
        run_id: "run-1".into(),
        unit_id: "run-1:unit:researcher".into(),
        attempt_id: "run-1:unit:researcher:attempt:1".into(),
        artifact_id: "artifact-tool-result".into(),
        tool_call_id: "tool-call-1".into(),
        tool_name: "git_diff".into(),
        tool_state: AutonomousToolCallStateDto::Succeeded,
        command_result: None,
        tool_summary: Some(sample_git_tool_summary()),
        action_id: Some("action-1".into()),
        boundary_id: Some("boundary-1".into()),
    })
    .expect("autonomous tool payload should serialize");
    assert_eq!(
        autonomous_tool_payload["toolSummary"],
        json!({
            "kind": "git",
            "scope": "worktree",
            "changedFiles": 3,
            "truncated": false,
            "baseRevision": "main~1"
        })
    );

    let browser_tool_payload = serde_json::to_value(AutonomousToolResultPayloadDto {
        project_id: "project-1".into(),
        run_id: "run-1".into(),
        unit_id: "run-1:unit:researcher".into(),
        attempt_id: "run-1:unit:researcher:attempt:1".into(),
        artifact_id: "artifact-browser-tool-result".into(),
        tool_call_id: "tool-call-browser-1".into(),
        tool_name: "browser".into(),
        tool_state: AutonomousToolCallStateDto::Succeeded,
        command_result: None,
        tool_summary: Some(sample_browser_computer_use_tool_summary()),
        action_id: Some("action-1".into()),
        boundary_id: Some("boundary-1".into()),
    })
    .expect("browser tool payload should serialize");
    assert_eq!(
        browser_tool_payload["toolSummary"],
        json!({
            "kind": "browser_computer_use",
            "surface": "browser",
            "action": "click",
            "status": "succeeded",
            "target": "button[type=submit]",
            "outcome": "Clicked submit and advanced to the confirmation view."
        })
    );

    let runtime_stream_tool_item = serde_json::to_value(RuntimeStreamItemDto {
        kind: RuntimeStreamItemKind::Tool,
        run_id: "run-1".into(),
        sequence: 8,
        session_id: Some("session-1".into()),
        flow_id: Some("flow-1".into()),
        text: None,
        tool_call_id: Some("tool-call-2".into()),
        tool_name: Some("web_fetch".into()),
        tool_state: Some(cadence_desktop_lib::commands::RuntimeToolCallState::Succeeded),
        tool_summary: Some(sample_web_tool_summary()),
        skill_id: None,
        skill_stage: None,
        skill_result: None,
        skill_source: None,
        skill_cache_status: None,
        skill_diagnostic: None,
        action_id: None,
        boundary_id: None,
        action_type: None,
        title: None,
        detail: Some("Fetched bounded web content.".into()),
        code: None,
        message: None,
        retryable: None,
        created_at: "2026-04-16T14:00:02Z".into(),
    })
    .expect("runtime stream tool item should serialize");
    assert_eq!(
        runtime_stream_tool_item["toolSummary"],
        json!({
            "kind": "web",
            "target": "https://example.com/search?q=Cadence",
            "resultCount": 5,
            "finalUrl": "https://example.com/search?q=Cadence",
            "contentKind": "html",
            "contentType": "text/html",
            "truncated": false
        })
    );

    let runtime_stream_skill_item = serde_json::to_value(RuntimeStreamItemDto {
        kind: RuntimeStreamItemKind::Skill,
        run_id: "run-1".into(),
        sequence: 9,
        session_id: Some("session-1".into()),
        flow_id: Some("flow-1".into()),
        text: None,
        tool_call_id: None,
        tool_name: None,
        tool_state: None,
        tool_summary: None,
        skill_id: Some("find-skills".into()),
        skill_stage: Some(AutonomousSkillLifecycleStageDto::Install),
        skill_result: Some(AutonomousSkillLifecycleResultDto::Succeeded),
        skill_source: Some(AutonomousSkillLifecycleSourceDto {
            repo: "vercel-labs/skills".into(),
            path: "skills/find-skills".into(),
            reference: "main".into(),
            tree_hash: "0123456789abcdef0123456789abcdef01234567".into(),
        }),
        skill_cache_status: Some(AutonomousSkillCacheStatusDto::Refreshed),
        skill_diagnostic: None,
        action_id: None,
        boundary_id: None,
        action_type: None,
        title: None,
        detail: Some(
            "Installed autonomous skill `find-skills` from the cached vercel-labs/skills tree."
                .into(),
        ),
        code: None,
        message: None,
        retryable: None,
        created_at: "2026-04-16T14:00:03Z".into(),
    })
    .expect("runtime stream skill item should serialize");
    assert_eq!(
        runtime_stream_skill_item,
        json!({
            "kind": "skill",
            "runId": "run-1",
            "sequence": 9,
            "sessionId": "session-1",
            "flowId": "flow-1",
            "text": null,
            "toolCallId": null,
            "toolName": null,
            "toolState": null,
            "actionId": null,
            "boundaryId": null,
            "actionType": null,
            "title": null,
            "detail": "Installed autonomous skill `find-skills` from the cached vercel-labs/skills tree.",
            "code": null,
            "message": null,
            "retryable": null,
            "createdAt": "2026-04-16T14:00:03Z",
            "skillId": "find-skills",
            "skillStage": "install",
            "skillResult": "succeeded",
            "skillSource": {
                "repo": "vercel-labs/skills",
                "path": "skills/find-skills",
                "reference": "main",
                "treeHash": "0123456789abcdef0123456789abcdef01234567"
            },
            "skillCacheStatus": "refreshed"
        })
    );
}

pub(crate) fn skill_lifecycle_payload_contracts_remain_tagged_and_camel_case() {
    assert_eq!(
        serde_json::to_value(sample_skill_lifecycle_payload())
            .expect("skill lifecycle payload should serialize"),
        json!({
            "projectId": "project-1",
            "runId": "run-1",
            "unitId": "run-1:unit:researcher",
            "attemptId": "run-1:unit:researcher:attempt:1",
            "artifactId": "artifact-skill-lifecycle",
            "stage": "install",
            "result": "failed",
            "skillId": "find-skills",
            "source": {
                "repo": "vercel-labs/skills",
                "path": "skills/find-skills",
                "reference": "main",
                "treeHash": "0123456789abcdef0123456789abcdef01234567"
            },
            "cache": {
                "key": "find-skills-576b45048241",
                "status": "refreshed"
            },
            "diagnostic": {
                "code": "autonomous_skill_cache_drift",
                "message": "Cadence detected autonomous skill cache drift for `find-skills`.",
                "retryable": false
            }
        })
    );

    let discovery_payload = serde_json::to_value(AutonomousSkillLifecyclePayloadDto {
        stage: AutonomousSkillLifecycleStageDto::Discovery,
        result: AutonomousSkillLifecycleResultDto::Succeeded,
        cache: AutonomousSkillLifecycleCacheDto {
            key: "find-skills-576b45048241".into(),
            status: None,
        },
        diagnostic: None,
        ..sample_skill_lifecycle_payload()
    })
    .expect("discovery skill lifecycle payload should serialize");
    assert_eq!(discovery_payload["stage"], json!("discovery"));
    assert!(
        discovery_payload.get("diagnostic").is_none(),
        "successful discovery payload should omit diagnostics"
    );
    assert_eq!(
        discovery_payload["cache"],
        json!({ "key": "find-skills-576b45048241" })
    );
}

pub(crate) fn serialization_stays_camel_case_for_responses_events_and_errors() {
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
                "name": "Cadence",
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
                "rootPath": "/tmp/Cadence",
                "displayName": "Cadence",
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
                "name": "Cadence",
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
                "rootPath": "/tmp/Cadence",
                "displayName": "Cadence",
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
                        "unblockReason": "discussion requires explicit plan-mode approval before implementation can continue.",
                        "unblockGateKey": "plan_mode_required",
                        "unblockActionId": "flow:flow-1:run:run-1:boundary:plan-mode:approve",
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
                    "detail": "Cadence currently has 3 changed file(s). Review the worktree before trusting subsequent agent actions.",
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
                "name": "Cadence",
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
            last_commit: None,
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
                    "rootPath": "/tmp/Cadence",
                    "displayName": "Cadence",
                    "branch": "main",
                    "headSha": "abc123",
                    "isGitRepo": true
                },
                "branch": {
                    "name": "main",
                    "headSha": "abc123",
                    "detached": false
                },
                "lastCommit": null,
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
                "rootPath": "/tmp/Cadence",
                "displayName": "Cadence",
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
            anthropic_api_key: None,
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

    let runtime_settings_response =
        serde_json::to_value(cadence_desktop_lib::commands::RuntimeSettingsDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key_configured: true,
            anthropic_api_key_configured: false,
        })
        .expect("runtime settings response should serialize");
    assert_eq!(
        runtime_settings_response,
        json!({
            "providerId": "openrouter",
            "modelId": "openai/gpt-4o-mini",
            "openrouterApiKeyConfigured": true
        })
    );

    let anthropic_runtime_settings_request = serde_json::to_value(
        cadence_desktop_lib::commands::UpsertRuntimeSettingsRequestDto {
            provider_id: "anthropic".into(),
            model_id: "claude-3-7-sonnet-latest".into(),
            openrouter_api_key: None,
            anthropic_api_key: Some("credential-value-ant-1".into()),
        },
    )
    .expect("anthropic runtime settings request should serialize");
    assert_eq!(
        anthropic_runtime_settings_request,
        json!({
            "providerId": "anthropic",
            "modelId": "claude-3-7-sonnet-latest",
            "anthropicApiKey": "credential-value-ant-1"
        })
    );

    let anthropic_runtime_settings_response =
        serde_json::to_value(cadence_desktop_lib::commands::RuntimeSettingsDto {
            provider_id: "anthropic".into(),
            model_id: "claude-3-7-sonnet-latest".into(),
            openrouter_api_key_configured: false,
            anthropic_api_key_configured: true,
        })
        .expect("anthropic runtime settings response should serialize");
    assert_eq!(
        anthropic_runtime_settings_response,
        json!({
            "providerId": "anthropic",
            "modelId": "claude-3-7-sonnet-latest",
            "openrouterApiKeyConfigured": false,
            "anthropicApiKeyConfigured": true
        })
    );

    let upsert_provider_profile_request = serde_json::to_value(UpsertProviderProfileRequestDto {
        profile_id: "openrouter-work".into(),
        provider_id: "openrouter".into(),
        runtime_kind: "openrouter".into(),
        label: "OpenRouter Work".into(),
        model_id: "openai/gpt-4.1-mini".into(),
        preset_id: Some("openrouter".into()),
        base_url: None,
        api_version: None,
        region: None,
        project_id: None,
        api_key: Some("credential-value-2".into()),
        activate: true,
    })
    .expect("provider profile request should serialize");
    assert_eq!(
        upsert_provider_profile_request,
        json!({
            "profileId": "openrouter-work",
            "providerId": "openrouter",
            "runtimeKind": "openrouter",
            "label": "OpenRouter Work",
            "modelId": "openai/gpt-4.1-mini",
            "presetId": "openrouter",
            "apiKey": "credential-value-2",
            "activate": true
        })
    );

    let anthropic_provider_profile_request =
        serde_json::to_value(UpsertProviderProfileRequestDto {
            profile_id: "anthropic-work".into(),
            provider_id: "anthropic".into(),
            runtime_kind: "anthropic".into(),
            label: "Anthropic Work".into(),
            model_id: "claude-3-7-sonnet-latest".into(),
            preset_id: Some("anthropic".into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: Some("credential-value-ant-2".into()),
            activate: false,
        })
        .expect("anthropic provider profile request should serialize");
    assert_eq!(
        anthropic_provider_profile_request,
        json!({
            "profileId": "anthropic-work",
            "providerId": "anthropic",
            "runtimeKind": "anthropic",
            "label": "Anthropic Work",
            "modelId": "claude-3-7-sonnet-latest",
            "presetId": "anthropic",
            "apiKey": "credential-value-ant-2",
            "activate": false
        })
    );

    let anthropic_launch_context = serde_json::to_value(RuntimeSupervisorLaunchContext {
        provider_id: "anthropic".into(),
        session_id: "anthropic-session-1".into(),
        flow_id: Some("flow-anthropic-1".into()),
        model_id: "claude-3-7-sonnet-latest".into(),
        thinking_effort: Some(ProviderModelThinkingEffortDto::High),
    })
    .expect("anthropic launch context should serialize");
    assert_eq!(
        anthropic_launch_context,
        json!({
            "providerId": "anthropic",
            "sessionId": "anthropic-session-1",
            "flowId": "flow-anthropic-1",
            "modelId": "claude-3-7-sonnet-latest",
            "thinkingEffort": "high"
        })
    );

    let set_active_profile_request = serde_json::to_value(SetActiveProviderProfileRequestDto {
        profile_id: "openrouter-work".into(),
    })
    .expect("active profile request should serialize");
    assert_eq!(
        set_active_profile_request,
        json!({ "profileId": "openrouter-work" })
    );

    let provider_profiles_response = serde_json::to_value(ProviderProfilesDto {
        active_profile_id: "openrouter-work".into(),
        profiles: vec![ProviderProfileDto {
            profile_id: "openrouter-work".into(),
            provider_id: "openrouter".into(),
            runtime_kind: "openrouter".into(),
            label: "OpenRouter Work".into(),
            model_id: "openai/gpt-4.1-mini".into(),
            preset_id: Some("openrouter".into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            active: true,
            readiness: ProviderProfileReadinessDto {
                ready: true,
                status: ProviderProfileReadinessStatusDto::Ready,
                proof: Some(
                    cadence_desktop_lib::commands::ProviderProfileReadinessProofDto::StoredSecret,
                ),
                proof_updated_at: Some("2026-04-21T02:00:05Z".into()),
            },
            migrated_from_legacy: true,
            migrated_at: Some("2026-04-21T02:00:00Z".into()),
        }],
        migration: Some(ProviderProfilesMigrationDto {
            source: "legacy_runtime_settings_v1".into(),
            migrated_at: "2026-04-21T02:00:00Z".into(),
            runtime_settings_updated_at: Some("2026-04-21T01:59:55Z".into()),
            openrouter_credentials_updated_at: Some("2026-04-21T02:00:05Z".into()),
            openai_auth_updated_at: None,
            openrouter_model_inferred: Some(false),
        }),
    })
    .expect("provider profiles response should serialize");
    assert_eq!(
        provider_profiles_response,
        json!({
            "activeProfileId": "openrouter-work",
            "profiles": [{
                "profileId": "openrouter-work",
                "providerId": "openrouter",
                "runtimeKind": "openrouter",
                "label": "OpenRouter Work",
                "modelId": "openai/gpt-4.1-mini",
                "presetId": "openrouter",
                "active": true,
                "readiness": {
                    "ready": true,
                    "status": "ready",
                    "proof": "stored_secret",
                    "proofUpdatedAt": "2026-04-21T02:00:05Z"
                },
                "migratedFromLegacy": true,
                "migratedAt": "2026-04-21T02:00:00Z"
            }],
            "migration": {
                "source": "legacy_runtime_settings_v1",
                "migratedAt": "2026-04-21T02:00:00Z",
                "runtimeSettingsUpdatedAt": "2026-04-21T01:59:55Z",
                "openrouterCredentialsUpdatedAt": "2026-04-21T02:00:05Z",
                "openaiAuthUpdatedAt": null,
                "openrouterModelInferred": false
            }
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
        controls: RuntimeRunControlStateDto {
            active: RuntimeRunActiveControlSnapshotDto {
                model_id: "openai_codex".into(),
                thinking_effort: Some(ProviderModelThinkingEffortDto::Medium),
                approval_mode: RuntimeRunApprovalModeDto::Suggest,
                plan_mode_required: true,
                revision: 1,
                applied_at: "2026-04-15T23:10:00Z".into(),
            },
            pending: Some(RuntimeRunPendingControlSnapshotDto {
                model_id: "openai_codex".into(),
                thinking_effort: Some(ProviderModelThinkingEffortDto::High),
                approval_mode: RuntimeRunApprovalModeDto::AutoEdit,
                plan_mode_required: true,
                revision: 2,
                queued_at: "2026-04-15T23:10:01Z".into(),
                queued_prompt: Some("Review the latest diff before continuing.".into()),
                queued_prompt_at: Some("2026-04-15T23:10:01Z".into()),
            }),
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
            "controls": {
                "active": {
                    "modelId": "openai_codex",
                    "thinkingEffort": "medium",
                    "approvalMode": "suggest",
                    "planModeRequired": true,
                    "revision": 1,
                    "appliedAt": "2026-04-15T23:10:00Z"
                },
                "pending": {
                    "modelId": "openai_codex",
                    "thinkingEffort": "high",
                    "approvalMode": "auto_edit",
                    "planModeRequired": true,
                    "revision": 2,
                    "queuedAt": "2026-04-15T23:10:01Z",
                    "queuedPrompt": "Review the latest diff before continuing.",
                    "queuedPromptAt": "2026-04-15T23:10:01Z"
                }
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
            controls: RuntimeRunControlStateDto {
                active: RuntimeRunActiveControlSnapshotDto {
                    model_id: "openai_codex".into(),
                    thinking_effort: Some(ProviderModelThinkingEffortDto::Medium),
                    approval_mode: RuntimeRunApprovalModeDto::Suggest,
                    plan_mode_required: false,
                    revision: 1,
                    applied_at: "2026-04-15T23:10:00Z".into(),
                },
                pending: None,
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
                "controls": {
                    "active": {
                        "modelId": "openai_codex",
                        "thinkingEffort": "medium",
                        "approvalMode": "suggest",
                        "planModeRequired": false,
                        "revision": 1,
                        "appliedAt": "2026-04-15T23:10:00Z"
                    }
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
        profile_id: "openai_codex-default".into(),
        originator: Some("Cadence-tests".into()),
    })
    .expect("auth start request should serialize");
    assert_eq!(
        start_request,
        json!({
            "projectId": "project-1",
            "profileId": "openai_codex-default",
            "originator": "Cadence-tests"
        })
    );

    let complete_request = serde_json::to_value(SubmitOpenAiCallbackRequestDto {
        project_id: "project-1".into(),
        profile_id: "openai_codex-default".into(),
        flow_id: "flow-1".into(),
        manual_input: Some("http://localhost:1455/auth/callback?code=abc".into()),
    })
    .expect("auth complete request should serialize");
    assert_eq!(
        complete_request,
        json!({
            "projectId": "project-1",
            "profileId": "openai_codex-default",
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
        initial_controls: Some(RuntimeRunControlInputDto {
            model_id: "openai_codex".into(),
            thinking_effort: Some(ProviderModelThinkingEffortDto::High),
            approval_mode: RuntimeRunApprovalModeDto::Yolo,
            plan_mode_required: true,
        }),
        initial_prompt: Some("Continue with the next verifier step.".into()),
    })
    .expect("start autonomous run request should serialize");
    assert_eq!(
        start_autonomous_run_request,
        json!({
            "projectId": "project-1",
            "initialControls": {
                "modelId": "openai_codex",
                "thinkingEffort": "high",
                "approvalMode": "yolo",
                "planModeRequired": true
            },
            "initialPrompt": "Continue with the next verifier step."
        })
    );

    let start_runtime_run_request = serde_json::to_value(StartRuntimeRunRequestDto {
        project_id: "project-1".into(),
        initial_controls: Some(RuntimeRunControlInputDto {
            model_id: "openai_codex".into(),
            thinking_effort: Some(ProviderModelThinkingEffortDto::Low),
            approval_mode: RuntimeRunApprovalModeDto::Suggest,
            plan_mode_required: false,
        }),
        initial_prompt: None,
    })
    .expect("start runtime run request should serialize");
    assert_eq!(
        start_runtime_run_request,
        json!({
            "projectId": "project-1",
            "initialControls": {
                "modelId": "openai_codex",
                "thinkingEffort": "low",
                "approvalMode": "suggest",
                "planModeRequired": false
            }
        })
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
            detail: "Cadence currently has 3 changed file(s). Review the worktree before trusting subsequent agent actions.".into(),
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
                "detail": "Cadence currently has 3 changed file(s). Review the worktree before trusting subsequent agent actions.",
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
            detail: "Cadence currently has 3 changed file(s). Review the worktree before trusting subsequent agent actions.".into(),
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
                "detail": "Cadence currently has 3 changed file(s). Review the worktree before trusting subsequent agent actions.",
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
            "skill".into(),
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
            "itemKinds": ["transcript", "tool", "skill", "activity", "failure"]
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
            RuntimeStreamItemKind::Skill,
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
            "subscribedItemKinds": ["transcript", "tool", "skill", "activity", "failure"]
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
        tool_summary: None,
        skill_id: None,
        skill_stage: None,
        skill_result: None,
        skill_source: None,
        skill_cache_status: None,
        skill_diagnostic: None,
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
            "skill",
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
    assert_eq!(
        cadence_desktop_lib::commands::LIST_PROVIDER_PROFILES_COMMAND,
        "list_provider_profiles"
    );
    assert_eq!(
        cadence_desktop_lib::commands::UPSERT_PROVIDER_PROFILE_COMMAND,
        "upsert_provider_profile"
    );
    assert_eq!(
        cadence_desktop_lib::commands::SET_ACTIVE_PROVIDER_PROFILE_COMMAND,
        "set_active_provider_profile"
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
