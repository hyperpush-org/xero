use super::support::*;

pub(crate) fn autonomous_fixture_repo_parity_binds_openrouter_truth_and_replays_tool_skill_recovery_after_reload(
) {
    let _guard = supervisor_test_guard();
    let models_base_url =
        spawn_static_http_server(200, r#"{"data":[{"id":"openai/gpt-4o-mini"}]}"#);
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_openrouter_state(
        &root,
        format!("{models_base_url}/api/v1/models"),
    ));
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_planning_lifecycle_workflow(&repo_root, &project_id);
    upsert_notification_route(
        &repo_root,
        &project_id,
        "route-telegram",
        "telegram",
        "telegram:ops-room",
    );

    let runtime_session =
        seed_openrouter_runtime(&app, &project_id, "sk-or-v1-openrouter-deterministic-proof");
    let runtime_session_id = runtime_session
        .session_id
        .clone()
        .expect("openrouter runtime session id should exist");
    assert!(runtime_session_id.starts_with("openrouter-session-"));
    assert!(runtime_session
        .account_id
        .as_deref()
        .is_some_and(|account_id| account_id.starts_with("openrouter-acct-")));

    let database_bytes =
        std::fs::read(database_path_for_repo(&repo_root)).expect("read runtime db bytes");
    let database_text = String::from_utf8_lossy(&database_bytes);
    assert!(!database_text.contains("sk-or-v1-openrouter-deterministic-proof"));

    let cache_root = app
        .state::<DesktopState>()
        .autonomous_skill_cache_dir(&app.handle().clone())
        .expect("autonomous skill cache dir");
    let source = FixtureSkillSource::default();
    source.set_tree_response(Ok(standard_skill_tree(
        "find-skills",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    )));
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/SKILL.md",
        "---\nname: find-skills\ndescription: Discover installable skills.\nuser-invocable: false\n---\n\n# Find Skills\n",
    );
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/guide.md",
        "Use this for discovery.\n",
    );

    let skill_runtime = AutonomousSkillRuntime::with_source_and_cache(
        skill_runtime_config(),
        Arc::new(source.clone()),
        Arc::new(FilesystemAutonomousSkillCacheStore::new(cache_root)),
    );

    let discovered = skill_runtime
        .discover(
            cadence_desktop_lib::runtime::AutonomousSkillDiscoverRequest {
                query: "find".into(),
                result_limit: Some(5),
                timeout_ms: Some(1_000),
                source_repo: None,
                source_ref: None,
            },
        )
        .expect("fixture discovery should succeed");
    let discovered_source = discovered
        .candidates
        .first()
        .expect("discovery should return one skill candidate")
        .source
        .clone();
    let installed = skill_runtime
        .install(
            cadence_desktop_lib::runtime::AutonomousSkillInstallRequest {
                source: discovered_source.clone(),
                timeout_ms: Some(1_000),
            },
        )
        .expect("fixture install should succeed");
    let invoked = skill_runtime
        .invoke(cadence_desktop_lib::runtime::AutonomousSkillInvokeRequest {
            source: discovered_source.clone(),
            timeout_ms: Some(1_000),
        })
        .expect("fixture invoke should reuse the Cadence cache");

    let skill_lines = vec![
        format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "skill",
                "skill_id": discovered.candidates[0].skill_id,
                "stage": "discovery",
                "result": "succeeded",
                "detail": "Resolved autonomous skill `find-skills` from the fixture vercel-labs/skills tree.",
                "source": {
                    "repo": discovered_source.repo,
                    "path": discovered_source.path,
                    "reference": discovered_source.reference,
                    "tree_hash": discovered_source.tree_hash,
                }
            })
        ),
        format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "skill",
                "skill_id": installed.skill_id,
                "stage": "install",
                "result": "succeeded",
                "detail": "Installed autonomous skill `find-skills` from the Cadence-owned fixture cache.",
                "source": {
                    "repo": installed.source.repo,
                    "path": installed.source.path,
                    "reference": installed.source.reference,
                    "tree_hash": installed.source.tree_hash,
                },
                "cache_status": "miss"
            })
        ),
        format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "skill",
                "skill_id": invoked.skill_id,
                "stage": "invoke",
                "result": "succeeded",
                "detail": "Invoked autonomous skill `find-skills` from the Cadence-owned fixture cache.",
                "source": {
                    "repo": invoked.source.repo,
                    "path": invoked.source.path,
                    "reference": invoked.source.reference,
                    "tree_hash": invoked.source.tree_hash,
                },
                "cache_status": "hit"
            })
        ),
    ];

    let launched = launch_scripted_runtime_run_with_runtime_kind(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        &runtime_session.runtime_kind,
        "run-openrouter-fixture-parity",
        &runtime_session_id,
        runtime_session.flow_id.as_deref(),
        &combined_fixture_story_script(&skill_lines),
    );

    wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.run_id == launched.run.run_id
            && runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });

    let progressed = wait_for_autonomous_run(&app, &project_id, |autonomous_state| {
        let Some(run) = autonomous_state.run.as_ref() else {
            return false;
        };
        let Some(unit) = autonomous_state.unit.as_ref() else {
            return false;
        };
        let Some(attempt) = autonomous_state.attempt.as_ref() else {
            return false;
        };
        let Some(linkage) = unit.workflow_linkage.as_ref() else {
            return false;
        };

        run.run_id == launched.run.run_id
            && autonomous_state.history.len() == 3
            && unit.sequence == 3
            && attempt.attempt_number == 3
            && unit.kind == AutonomousUnitKindDto::Planner
            && linkage.workflow_node_id == "roadmap"
            && attempt.workflow_linkage.as_ref() == Some(linkage)
    });
    let progressed_shape = history_shape(&progressed);

    thread::sleep(Duration::from_secs(3));
    let boundary_id = "boundary-1".to_string();
    let persisted_boundary = project_store::upsert_runtime_action_required(
        &repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.clone(),
            run_id: launched.run.run_id.clone(),
            runtime_kind: launched.run.runtime_kind.clone(),
            session_id: runtime_session_id.clone(),
            flow_id: runtime_session.flow_id.clone(),
            transport_endpoint: launched.run.transport.endpoint.clone(),
            started_at: launched.run.started_at.clone(),
            last_heartbeat_at: launched.run.last_heartbeat_at.clone(),
            last_error: None,
            boundary_id: boundary_id.clone(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
            checkpoint_summary:
                "Detached runtime blocked on terminal input and is awaiting operator approval."
                    .into(),
            created_at: "2026-04-18T19:00:00Z".into(),
        },
    )
    .expect("persist runtime action-required boundary for openrouter fixture parity proof");
    let persisted_action_id = persisted_boundary.approval_request.action_id.clone();
    persist_supervisor_event(
        &repo_root,
        &project_id,
        &SupervisorLiveEventPayload::ActionRequired {
            action_id: persisted_action_id.clone(),
            boundary_id: boundary_id.clone(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
        },
    )
    .expect("persist autonomous action-required event for openrouter fixture parity proof")
    .expect("openrouter autonomous action-required persistence should return a snapshot");

    let paused = wait_for_autonomous_run(&app, &project_id, |autonomous_state| {
        let Some(run) = autonomous_state.run.as_ref() else {
            return false;
        };
        let Some(unit) = autonomous_state.unit.as_ref() else {
            return false;
        };
        let Some(attempt) = autonomous_state.attempt.as_ref() else {
            return false;
        };

        let Some(current_entry) = autonomous_state.history.first() else {
            return false;
        };
        let current_attempt_id = attempt.attempt_id.as_str();
        let tool_count = current_entry
            .artifacts
            .iter()
            .filter(|artifact| {
                artifact.attempt_id == current_attempt_id && artifact.artifact_kind == "tool_result"
            })
            .count();
        let skill_count = current_entry
            .artifacts
            .iter()
            .filter(|artifact| {
                artifact.attempt_id == current_attempt_id
                    && artifact.artifact_kind == "skill_lifecycle"
            })
            .count();
        let verification_count = current_entry
            .artifacts
            .iter()
            .filter(|artifact| {
                artifact.attempt_id == current_attempt_id
                    && artifact.artifact_kind == "verification_evidence"
            })
            .count();
        let policy_count = current_entry
            .artifacts
            .iter()
            .filter(|artifact| {
                artifact.attempt_id == current_attempt_id
                    && artifact.artifact_kind == "policy_denied"
            })
            .count();

        run.run_id == launched.run.run_id
            && run.status == AutonomousRunStatusDto::Paused
            && unit.status == AutonomousUnitStatusDto::Blocked
            && attempt.status == AutonomousUnitStatusDto::Blocked
            && unit.boundary_id == attempt.boundary_id
            && history_shape(autonomous_state) == progressed_shape
            && tool_count == 2
            && skill_count == 3
            && verification_count == 1
            && policy_count == 1
    });
    let paused_attempt = paused
        .attempt
        .as_ref()
        .expect("paused autonomous attempt should exist");
    let boundary_id = paused_attempt
        .boundary_id
        .clone()
        .expect("paused autonomous attempt should carry the runtime boundary id");
    let paused_shape = history_shape(&paused);
    assert_eq!(paused_shape, progressed_shape);

    let pending_snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after openrouter autonomous boundary pause");
    assert_eq!(pending_snapshot.approval_requests.len(), 1);
    assert!(pending_snapshot.resume_history.is_empty());
    let action_id = pending_snapshot.approval_requests[0].action_id.clone();
    assert!(action_id.contains(&format!(":boundary:{boundary_id}:")));
    assert_eq!(
        pending_snapshot.approval_requests[0].status,
        OperatorApprovalStatus::Pending
    );

    let dispatches =
        wait_for_notification_dispatches_for_action(&repo_root, &project_id, &action_id, 1);
    let telegram_dispatch = dispatches
        .first()
        .expect("telegram dispatch row should exist")
        .clone();

    let paused_durable = project_store::load_autonomous_run(&repo_root, &project_id)
        .expect("load durable paused autonomous run")
        .expect("durable paused autonomous run should exist");
    let paused_artifacts = paused_durable
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .filter(|artifact| artifact.attempt_id == paused_attempt.attempt_id)
        .collect::<Vec<_>>();
    assert_eq!(paused_artifacts.len(), 7);
    assert_eq!(
        paused_artifacts
            .iter()
            .filter(|artifact| artifact.artifact_kind == "tool_result")
            .count(),
        2
    );
    assert_eq!(
        paused_artifacts
            .iter()
            .filter(|artifact| artifact.artifact_kind == "skill_lifecycle")
            .count(),
        3
    );
    assert_eq!(
        paused_artifacts
            .iter()
            .filter(|artifact| artifact.artifact_kind == "policy_denied")
            .count(),
        1
    );
    assert_eq!(
        paused_artifacts
            .iter()
            .filter(|artifact| artifact.artifact_kind == "verification_evidence")
            .count(),
        1
    );
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(payload))
                if payload.tool_call_id == "tool-inspect-1"
                    && payload.tool_name == "inspect_repository"
                    && payload.tool_state == project_store::AutonomousToolCallStateRecord::Running
        )
    }));
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(payload))
                if payload.tool_call_id == "tool-inspect-1"
                    && payload.tool_name == "inspect_repository"
                    && payload.tool_state == project_store::AutonomousToolCallStateRecord::Succeeded
        )
    }));
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::SkillLifecycle(payload))
                if payload.stage == project_store::AutonomousSkillLifecycleStageRecord::Discovery
                    && payload.result == project_store::AutonomousSkillLifecycleResultRecord::Succeeded
                    && payload.skill_id == "find-skills"
                    && payload.source.repo == "vercel-labs/skills"
                    && payload.source.path == "skills/find-skills"
                    && payload.source.reference == "main"
                    && payload.source.tree_hash == "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    && payload.cache.status.is_none()
        )
    }));
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::SkillLifecycle(payload))
                if payload.stage == project_store::AutonomousSkillLifecycleStageRecord::Install
                    && payload.result == project_store::AutonomousSkillLifecycleResultRecord::Succeeded
                    && payload.cache.status
                        == Some(project_store::AutonomousSkillCacheStatusRecord::Miss)
        )
    }));
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::SkillLifecycle(payload))
                if payload.stage == project_store::AutonomousSkillLifecycleStageRecord::Invoke
                    && payload.result == project_store::AutonomousSkillLifecycleResultRecord::Succeeded
                    && payload.cache.status
                        == Some(project_store::AutonomousSkillCacheStatusRecord::Hit)
        )
    }));
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::PolicyDenied(payload))
                if payload.diagnostic_code == "policy_denied_write_access"
                    && payload.message == "Cadence blocked repository writes until operator approval resumes the active boundary"
        )
    }));
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                if payload.action_id.as_deref() == Some(action_id.as_str())
                    && payload.boundary_id.as_deref() == Some(boundary_id.as_str())
                    && payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Blocked
                    && payload.evidence_kind == "terminal_input_required"
        )
    }));

    let payload_jsons = load_skill_payload_jsons(&repo_root);
    assert_eq!(payload_jsons.len(), 3);
    assert!(payload_jsons
        .iter()
        .all(|payload| !payload.contains("# Find Skills")));
    assert!(payload_jsons
        .iter()
        .all(|payload| !payload.contains("Use this for discovery.")));
    assert!(payload_jsons
        .iter()
        .all(|payload| !payload.contains("SKILL.md")));

    let fresh_paused_app = build_mock_app(create_state(&root));
    let recovered_paused = wait_for_autonomous_run(&fresh_paused_app, &project_id, |autonomous| {
        let Some(run) = autonomous.run.as_ref() else {
            return false;
        };
        let Some(attempt) = autonomous.attempt.as_ref() else {
            return false;
        };
        run.run_id == launched.run.run_id
            && run.status == AutonomousRunStatusDto::Paused
            && attempt.boundary_id.as_deref() == Some(boundary_id.as_str())
            && history_shape(autonomous) == paused_shape
    });
    assert_eq!(history_shape(&recovered_paused), paused_shape);

    let approved = submit_notification_reply(
        fresh_paused_app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: action_id.clone(),
            route_id: telegram_dispatch.route_id.clone(),
            correlation_key: telegram_dispatch.correlation_key.clone(),
            responder_id: Some("telegram-operator".into()),
            reply_text: "approved".into(),
            decision: "approve".into(),
            received_at: "2026-04-18T19:00:07Z".into(),
        },
    )
    .expect("remote reply should resume the openrouter parity run");
    assert_eq!(
        approved.claim.status,
        NotificationReplyClaimStatusDto::Accepted
    );
    assert_eq!(
        approved.resolve_result.approval_request.status,
        OperatorApprovalStatus::Approved
    );
    assert_eq!(
        approved
            .resume_result
            .as_ref()
            .map(|resume| resume.resume_entry.status.clone()),
        Some(ResumeHistoryStatus::Started)
    );

    let resumed = wait_for_autonomous_run(&fresh_paused_app, &project_id, |autonomous| {
        let Some(run) = autonomous.run.as_ref() else {
            return false;
        };
        let Some(attempt) = autonomous.attempt.as_ref() else {
            return false;
        };
        run.run_id == launched.run.run_id
            && run.status == AutonomousRunStatusDto::Running
            && attempt.boundary_id.is_none()
            && history_shape(autonomous) == paused_shape
    });
    assert_eq!(history_shape(&resumed), paused_shape);

    let resumed_runtime = wait_for_runtime_run(&fresh_paused_app, &project_id, |runtime_run| {
        runtime_run.run_id == launched.run.run_id
            && runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
            && runtime_run
                .checkpoints
                .iter()
                .any(|checkpoint| checkpoint.kind == RuntimeRunCheckpointKindDto::ActionRequired)
    });
    assert_eq!(resumed_runtime.run_id, launched.run.run_id);

    let resumed_durable = project_store::load_autonomous_run(&repo_root, &project_id)
        .expect("load durable autonomous run after resume")
        .expect("durable autonomous run should still exist after resume");
    let resumed_artifacts = resumed_durable
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .filter(|artifact| artifact.attempt_id == paused_attempt.attempt_id)
        .collect::<Vec<_>>();
    assert_eq!(resumed_artifacts.len(), 8);
    let boundary_evidence = resumed_artifacts
        .iter()
        .filter(|artifact| artifact.artifact_kind == "verification_evidence")
        .collect::<Vec<_>>();
    assert_eq!(boundary_evidence.len(), 2);
    assert_eq!(
        boundary_evidence
            .iter()
            .filter(|artifact| {
                matches!(
                    artifact.payload.as_ref(),
                    Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                        if payload.action_id.as_deref() == Some(action_id.as_str())
                            && payload.boundary_id.as_deref() == Some(boundary_id.as_str())
                            && payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Blocked
                            && payload.evidence_kind == "terminal_input_required"
                )
            })
            .count(),
        1
    );
    assert_eq!(
        boundary_evidence
            .iter()
            .filter(|artifact| {
                matches!(
                    artifact.payload.as_ref(),
                    Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                        if payload.action_id.as_deref() == Some(action_id.as_str())
                            && payload.boundary_id.as_deref() == Some(boundary_id.as_str())
                            && payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Passed
                            && payload.evidence_kind == "operator_resume"
                )
            })
            .count(),
        1
    );

    let resumed_snapshot = get_project_snapshot(
        fresh_paused_app.handle().clone(),
        fresh_paused_app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after remote resume");
    assert_eq!(resumed_snapshot.resume_history.len(), 1);
    assert_eq!(
        resumed_snapshot.resume_history[0].status,
        ResumeHistoryStatus::Started
    );
    assert_eq!(
        resumed_snapshot.resume_history[0]
            .source_action_id
            .as_deref(),
        Some(action_id.as_str())
    );

    let replay_models_base_url =
        spawn_static_http_server_with_requests(200, r#"{"data":[{"id":"openai/gpt-4o-mini"}]}"#, 2);
    let replay_app = build_mock_app(create_openrouter_state(
        &root,
        format!("{replay_models_base_url}/api/v1/models"),
    ));
    let replay_runtime = seed_openrouter_runtime(
        &replay_app,
        &project_id,
        "sk-or-v1-openrouter-deterministic-proof",
    );
    assert_eq!(replay_runtime.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(replay_runtime.provider_id, "openrouter");
    assert_eq!(replay_runtime.runtime_kind, "openrouter");
    assert_eq!(replay_runtime.session_id, runtime_session.session_id);
    assert_eq!(replay_runtime.account_id, runtime_session.account_id);

    let replayed = wait_for_autonomous_run(&replay_app, &project_id, |autonomous| {
        let Some(run) = autonomous.run.as_ref() else {
            return false;
        };
        let Some(attempt) = autonomous.attempt.as_ref() else {
            return false;
        };
        run.run_id == launched.run.run_id
            && attempt.boundary_id.is_none()
            && history_shape(autonomous) == paused_shape
    });
    assert_eq!(history_shape(&replayed), paused_shape);

    let (channel, receiver) = capture_stream_channel();
    start_direct_runtime_stream(
        &replay_app,
        &project_id,
        &repo_root,
        &replay_runtime,
        &launched.run.run_id,
        vec![
            RuntimeStreamItemKind::Tool,
            RuntimeStreamItemKind::Skill,
            RuntimeStreamItemKind::Complete,
        ],
        channel,
    );

    let items = collect_until_terminal(receiver);
    assert_monotonic_sequences(&items, &launched.run.run_id);
    assert_eq!(
        items
            .iter()
            .map(|item| item.kind.clone())
            .collect::<Vec<_>>(),
        vec![
            RuntimeStreamItemKind::Tool,
            RuntimeStreamItemKind::Tool,
            RuntimeStreamItemKind::Skill,
            RuntimeStreamItemKind::Skill,
            RuntimeStreamItemKind::Skill,
            RuntimeStreamItemKind::Complete,
        ],
        "unexpected replayed openrouter parity items: {items:?}"
    );
    assert!(matches!(
        &items[0],
        RuntimeStreamItemDto {
            kind: RuntimeStreamItemKind::Tool,
            tool_call_id: Some(tool_call_id),
            tool_name: Some(tool_name),
            detail: Some(detail),
            ..
        } if tool_call_id == "tool-inspect-1"
            && tool_name == "inspect_repository"
            && detail == "Collecting deterministic fixture proof context"
    ));
    assert!(matches!(
        &items[1],
        RuntimeStreamItemDto {
            kind: RuntimeStreamItemKind::Tool,
            tool_call_id: Some(tool_call_id),
            tool_name: Some(tool_name),
            detail: Some(detail),
            ..
        } if tool_call_id == "tool-inspect-1"
            && tool_name == "inspect_repository"
            && detail == "Collected deterministic fixture proof context"
    ));
    assert_eq!(items[2].skill_id.as_deref(), Some("find-skills"));
    assert_eq!(
        items[2].skill_stage,
        Some(AutonomousSkillLifecycleStageDto::Discovery)
    );
    assert_eq!(
        items[3].skill_stage,
        Some(AutonomousSkillLifecycleStageDto::Install)
    );
    assert_eq!(
        items[4].skill_stage,
        Some(AutonomousSkillLifecycleStageDto::Invoke)
    );
    assert_eq!(
        items[3].skill_cache_status,
        Some(AutonomousSkillCacheStatusDto::Miss)
    );
    assert_eq!(
        items[4].skill_cache_status,
        Some(AutonomousSkillCacheStatusDto::Hit)
    );
    assert!(matches!(
        &items[5],
        RuntimeStreamItemDto {
            kind: RuntimeStreamItemKind::Complete,
            detail: Some(detail),
            ..
        } if detail.contains("finished")
    ));

    let final_runtime = wait_for_runtime_run(&replay_app, &project_id, |runtime_run| {
        runtime_run.run_id == launched.run.run_id
            && runtime_run.status == RuntimeRunStatusDto::Stopped
    });
    assert_eq!(final_runtime.status, RuntimeRunStatusDto::Stopped);
}

pub(crate) fn autonomous_fixture_repo_parity_replays_fixture_driven_skill_lifecycle_after_reload() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let runtime_session = seed_authenticated_runtime(&app, &root, &project_id);
    let cache_root = app
        .state::<DesktopState>()
        .autonomous_skill_cache_dir(&app.handle().clone())
        .expect("autonomous skill cache dir");

    let source = FixtureSkillSource::default();
    source.set_tree_response(Ok(standard_skill_tree(
        "find-skills",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )));
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/SKILL.md",
        "---\nname: find-skills\ndescription: Discover installable skills.\nuser-invocable: false\n---\n\n# Find Skills\n",
    );
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/guide.md",
        "Use this for discovery.\n",
    );

    let skill_runtime = AutonomousSkillRuntime::with_source_and_cache(
        skill_runtime_config(),
        Arc::new(source.clone()),
        Arc::new(FilesystemAutonomousSkillCacheStore::new(cache_root.clone())),
    );

    let discovered = skill_runtime
        .discover(
            cadence_desktop_lib::runtime::AutonomousSkillDiscoverRequest {
                query: "find".into(),
                result_limit: Some(5),
                timeout_ms: Some(1_000),
                source_repo: None,
                source_ref: None,
            },
        )
        .expect("fixture discovery should succeed");
    let discovered_source = discovered
        .candidates
        .first()
        .expect("discovery should return one skill candidate")
        .source
        .clone();
    let installed = skill_runtime
        .install(
            cadence_desktop_lib::runtime::AutonomousSkillInstallRequest {
                source: discovered_source.clone(),
                timeout_ms: Some(1_000),
            },
        )
        .expect("fixture install should succeed");
    let invoked = skill_runtime
        .invoke(cadence_desktop_lib::runtime::AutonomousSkillInvokeRequest {
            source: discovered_source.clone(),
            timeout_ms: Some(1_000),
        })
        .expect("fixture invoke should reuse the Cadence cache");

    assert_eq!(
        installed.cache_status,
        cadence_desktop_lib::runtime::AutonomousSkillCacheStatus::Miss
    );
    assert_eq!(
        invoked.cache_status,
        cadence_desktop_lib::runtime::AutonomousSkillCacheStatus::Hit
    );
    assert_eq!(source.tree_request_count(), 2);
    assert_eq!(source.file_request_count(), 2);
    assert!(Path::new(&installed.cache_directory).starts_with(&cache_root));
    assert!(Path::new(&invoked.cache_directory).starts_with(&cache_root));

    let skill_lines = vec![
        format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "skill",
                "skill_id": discovered.candidates[0].skill_id,
                "stage": "discovery",
                "result": "succeeded",
                "detail": "Resolved autonomous skill `find-skills` from the fixture vercel-labs/skills tree.",
                "source": {
                    "repo": discovered_source.repo,
                    "path": discovered_source.path,
                    "reference": discovered_source.reference,
                    "tree_hash": discovered_source.tree_hash,
                }
            })
        ),
        format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "skill",
                "skill_id": installed.skill_id,
                "stage": "install",
                "result": "succeeded",
                "detail": "Installed autonomous skill `find-skills` from the Cadence-owned fixture cache.",
                "source": {
                    "repo": installed.source.repo,
                    "path": installed.source.path,
                    "reference": installed.source.reference,
                    "tree_hash": installed.source.tree_hash,
                },
                "cache_status": "miss"
            })
        ),
        format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "skill",
                "skill_id": invoked.skill_id,
                "stage": "invoke",
                "result": "succeeded",
                "detail": "Invoked autonomous skill `find-skills` from the Cadence-owned fixture cache.",
                "source": {
                    "repo": invoked.source.repo,
                    "path": invoked.source.path,
                    "reference": invoked.source.reference,
                    "tree_hash": invoked.source.tree_hash,
                },
                "cache_status": "hit"
            })
        ),
    ];

    let launched = launch_scripted_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        "run-skill-fixture-parity",
        runtime_session
            .session_id
            .as_deref()
            .expect("authenticated runtime session id"),
        runtime_session.flow_id.as_deref(),
        &runtime_shell::script_join_steps(&[
            runtime_shell::script_print_line(&skill_lines[0]),
            runtime_shell::script_sleep(1),
            runtime_shell::script_print_line(&skill_lines[1]),
            runtime_shell::script_sleep(1),
            runtime_shell::script_print_line(&skill_lines[2]),
            runtime_shell::script_sleep(3),
        ]),
    );

    let observed = wait_for_autonomous_run(&app, &project_id, |autonomous_state| {
        let Some(run) = autonomous_state.run.as_ref() else {
            return false;
        };

        run.run_id == launched.run.run_id
            && autonomous_state.history.first().is_some_and(|entry| {
                entry
                    .artifacts
                    .iter()
                    .filter(|artifact| artifact.artifact_kind == "skill_lifecycle")
                    .count()
                    == 3
            })
    });
    let observed_skill_count = observed
        .history
        .first()
        .expect("observed autonomous history entry")
        .artifacts
        .iter()
        .filter(|artifact| artifact.artifact_kind == "skill_lifecycle")
        .count();
    assert_eq!(observed_skill_count, 3);

    let durable = project_store::load_autonomous_run(&repo_root, &project_id)
        .expect("load durable autonomous run after fixture skill story")
        .expect("durable autonomous run should exist after fixture skill story");
    let skill_artifacts = durable
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .filter(|artifact| artifact.artifact_kind == "skill_lifecycle")
        .collect::<Vec<_>>();
    assert_eq!(skill_artifacts.len(), 3);
    assert!(skill_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::SkillLifecycle(payload))
                if payload.stage == project_store::AutonomousSkillLifecycleStageRecord::Discovery
                    && payload.result == project_store::AutonomousSkillLifecycleResultRecord::Succeeded
                    && payload.skill_id == "find-skills"
                    && payload.source.repo == "vercel-labs/skills"
                    && payload.source.path == "skills/find-skills"
                    && payload.source.reference == "main"
                    && payload.source.tree_hash == "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    && payload.cache.status.is_none()
        )
    }));
    assert!(skill_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::SkillLifecycle(payload))
                if payload.stage == project_store::AutonomousSkillLifecycleStageRecord::Install
                    && payload.result == project_store::AutonomousSkillLifecycleResultRecord::Succeeded
                    && payload.cache.status
                        == Some(project_store::AutonomousSkillCacheStatusRecord::Miss)
        )
    }));
    assert!(skill_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::SkillLifecycle(payload))
                if payload.stage == project_store::AutonomousSkillLifecycleStageRecord::Invoke
                    && payload.result == project_store::AutonomousSkillLifecycleResultRecord::Succeeded
                    && payload.cache.status
                        == Some(project_store::AutonomousSkillCacheStatusRecord::Hit)
        )
    }));

    let payload_jsons = load_skill_payload_jsons(&repo_root);
    assert_eq!(payload_jsons.len(), 3);
    assert!(payload_jsons
        .iter()
        .all(|payload| !payload.contains("# Find Skills")));
    assert!(payload_jsons
        .iter()
        .all(|payload| !payload.contains("Use this for discovery.")));
    assert!(payload_jsons
        .iter()
        .all(|payload| !payload.contains("SKILL.md")));

    let fresh_app = build_mock_app(create_state(&root));
    let fresh_runtime = seed_authenticated_runtime(&fresh_app, &root, &project_id);
    let reloaded = wait_for_autonomous_run(&fresh_app, &project_id, |autonomous_state| {
        let Some(run) = autonomous_state.run.as_ref() else {
            return false;
        };
        run.run_id == launched.run.run_id
            && autonomous_state.history.first().is_some_and(|entry| {
                entry
                    .artifacts
                    .iter()
                    .filter(|artifact| artifact.artifact_kind == "skill_lifecycle")
                    .count()
                    == 3
            })
    });
    assert_eq!(
        reloaded
            .history
            .first()
            .expect("reloaded autonomous history entry")
            .artifacts
            .iter()
            .filter(|artifact| artifact.artifact_kind == "skill_lifecycle")
            .count(),
        3
    );

    let (channel, receiver) = capture_stream_channel();
    start_direct_runtime_stream(
        &fresh_app,
        &project_id,
        &repo_root,
        &fresh_runtime,
        &launched.run.run_id,
        vec![
            RuntimeStreamItemKind::Skill,
            RuntimeStreamItemKind::Complete,
        ],
        channel,
    );

    let items = collect_until_terminal(receiver);
    assert_monotonic_sequences(&items, &launched.run.run_id);
    assert_eq!(
        items
            .iter()
            .map(|item| item.kind.clone())
            .collect::<Vec<_>>(),
        vec![
            RuntimeStreamItemKind::Skill,
            RuntimeStreamItemKind::Skill,
            RuntimeStreamItemKind::Skill,
            RuntimeStreamItemKind::Complete,
        ],
        "unexpected replayed fixture skill items: {items:?}"
    );

    assert_eq!(items[0].skill_id.as_deref(), Some("find-skills"));
    assert_eq!(
        items[0].skill_stage,
        Some(AutonomousSkillLifecycleStageDto::Discovery)
    );
    assert_eq!(
        items[0].skill_result,
        Some(AutonomousSkillLifecycleResultDto::Succeeded)
    );
    assert_eq!(items[0].skill_cache_status, None);
    assert_eq!(
        items[0].detail.as_deref(),
        Some("Resolved autonomous skill `find-skills` from the fixture vercel-labs/skills tree.")
    );

    assert_eq!(items[1].skill_id.as_deref(), Some("find-skills"));
    assert_eq!(
        items[1].skill_stage,
        Some(AutonomousSkillLifecycleStageDto::Install)
    );
    assert_eq!(
        items[1].skill_result,
        Some(AutonomousSkillLifecycleResultDto::Succeeded)
    );
    assert_eq!(
        items[1].skill_cache_status,
        Some(AutonomousSkillCacheStatusDto::Miss)
    );
    assert_eq!(
        items[1].detail.as_deref(),
        Some("Installed autonomous skill `find-skills` from the Cadence-owned fixture cache.")
    );

    assert_eq!(items[2].skill_id.as_deref(), Some("find-skills"));
    assert_eq!(
        items[2].skill_stage,
        Some(AutonomousSkillLifecycleStageDto::Invoke)
    );
    assert_eq!(
        items[2].skill_result,
        Some(AutonomousSkillLifecycleResultDto::Succeeded)
    );
    assert_eq!(
        items[2].skill_cache_status,
        Some(AutonomousSkillCacheStatusDto::Hit)
    );
    assert_eq!(
        items[2].detail.as_deref(),
        Some("Invoked autonomous skill `find-skills` from the Cadence-owned fixture cache.")
    );
    assert!(items[2].skill_diagnostic.is_none());

    let final_runtime = wait_for_runtime_run(&fresh_app, &project_id, |runtime_run| {
        runtime_run.run_id == launched.run.run_id
            && runtime_run.status == RuntimeRunStatusDto::Stopped
    });
    assert_eq!(final_runtime.status, RuntimeRunStatusDto::Stopped);
}
