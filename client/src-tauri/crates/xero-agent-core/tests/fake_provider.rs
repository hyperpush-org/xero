use xero_agent_core::{
    provider_capability_catalog, provider_preflight_snapshot, runtime_trace_id_for_run,
    AgentCoreStore, AgentProtocolRuntime, AgentRuntimeFacade, ApprovalDecisionRequest,
    CancelRunRequest, CompactSessionRequest, ContinueRunRequest, ExportTraceRequest,
    FakeProviderRuntime, ForkSessionRequest, HeadlessProviderExecutionConfig,
    HeadlessProviderRuntime, HeadlessRuntimeOptions, InMemoryAgentCoreStore, NewRunRecord,
    OpenAiCompatibleHeadlessConfig, ProviderCapabilityCatalogInput, ProviderModelChangeRequest,
    ProviderPreflightInput, ProviderPreflightRequiredFeatures, ProviderPreflightSource,
    ProviderSelection, ResumeRunRequest, RunControls, RunStatus, RuntimeEventKind,
    RuntimeSettingsChangeRequest, RuntimeSubmission, RuntimeSubmissionEnvelope,
    RuntimeTraceContext, StartRunRequest, ToolPermissionGrantRequest, UserInputRequest,
    CORE_PROTOCOL_VERSION,
};

#[test]
fn non_tauri_fake_provider_run_receives_desktop_visible_events() {
    let runtime = FakeProviderRuntime::default();

    let snapshot = runtime
        .start_run(StartRunRequest {
            project_id: "project-1".into(),
            agent_session_id: "session-1".into(),
            run_id: "run-1".into(),
            prompt: "Start a fake-provider run from outside Tauri.".into(),
            provider: ProviderSelection {
                provider_id: "fake_provider".into(),
                model_id: "fake-model".into(),
            },
            controls: Some(RunControls {
                runtime_agent_id: "engineer".into(),
                agent_definition_id: Some("engineer".into()),
                agent_definition_version: Some(1),
                agent_definition_snapshot: None,
                thinking_effort: None,
                approval_mode: "yolo".into(),
                plan_mode_required: false,
            }),
        })
        .expect("fake provider run should complete");

    assert_eq!(snapshot.status, RunStatus::Completed);
    assert!(snapshot.events.iter().any(|event| event.event_kind
        == RuntimeEventKind::EnvironmentLifecycleUpdate
        && event
            .payload
            .get("state")
            .and_then(serde_json::Value::as_str)
            == Some("ready")));

    let manifest = snapshot
        .context_manifests
        .first()
        .expect("context manifest should be persisted before provider turn");
    let recorded_after_event_id = manifest
        .recorded_after_event_id
        .expect("manifest should record its event boundary");
    assert!(
        snapshot
            .events
            .iter()
            .find(|event| event.event_kind == RuntimeEventKind::ReasoningSummary)
            .expect("provider reasoning event")
            .id
            > recorded_after_event_id,
        "provider events must follow mandatory context manifest persistence"
    );
    assert!(snapshot.events.iter().all(|event| event.trace.is_valid()));
}

#[test]
fn protocol_submission_trace_replays_run_timeline() {
    let runtime = FakeProviderRuntime::default();
    let trace_id = runtime_trace_id_for_run("project-1", "run-1");

    let outcome = runtime
        .submit_protocol(RuntimeSubmissionEnvelope {
            protocol_version: CORE_PROTOCOL_VERSION,
            submission_id: "submission-1".into(),
            trace: RuntimeTraceContext::for_run(&trace_id, "run-1", "start_run"),
            submitted_at: "2026-05-03T12:00:00Z".into(),
            submission: RuntimeSubmission::StartRun(StartRunRequest {
                project_id: "project-1".into(),
                agent_session_id: "session-1".into(),
                run_id: "run-1".into(),
                prompt: "Start a fake-provider run through the typed protocol.".into(),
                provider: ProviderSelection {
                    provider_id: "fake_provider".into(),
                    model_id: "fake-model".into(),
                },
                controls: Some(RunControls {
                    runtime_agent_id: "engineer".into(),
                    agent_definition_id: Some("engineer".into()),
                    agent_definition_version: Some(1),
                    agent_definition_snapshot: None,
                    thinking_effort: None,
                    approval_mode: "yolo".into(),
                    plan_mode_required: false,
                }),
            }),
        })
        .expect("protocol submission should be accepted");

    let snapshot = outcome.snapshot.expect("start run returns snapshot");
    let trace = runtime
        .export_trace(xero_agent_core::ExportTraceRequest {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
        })
        .expect("trace export should succeed");
    let timeline = trace
        .replay_timeline()
        .expect("protocol trace should replay");

    assert_eq!(timeline.status, RunStatus::Completed);
    assert_eq!(
        timeline.items.first().unwrap().event_kind,
        RuntimeEventKind::RunStarted
    );
    assert!(timeline
        .items
        .iter()
        .any(|item| item.event_kind == RuntimeEventKind::ContextManifestRecorded));
    assert!(timeline.items.iter().all(|item| item.trace.is_valid()));
}

#[test]
fn protocol_rejects_blank_submission_identity_before_dispatch() {
    let runtime = FakeProviderRuntime::default();
    let trace_id = runtime_trace_id_for_run("project-1", "run-envelope-validation");

    for (submission_id, submitted_at, field) in [
        (" \n ", "2026-07-18T12:00:00Z", "submissionId"),
        ("submission-1", "\t", "submittedAt"),
    ] {
        let error = runtime
            .submit_protocol(RuntimeSubmissionEnvelope {
                protocol_version: CORE_PROTOCOL_VERSION,
                submission_id: submission_id.into(),
                trace: RuntimeTraceContext::for_run(
                    &trace_id,
                    "run-envelope-validation",
                    "runtime_settings_change",
                ),
                submitted_at: submitted_at.into(),
                submission: RuntimeSubmission::RuntimeSettingsChange(
                    RuntimeSettingsChangeRequest {
                        project_id: Some("project-1".into()),
                        settings: serde_json::json!({"approvalMode": "suggest"}),
                        reason: Some("fixture".into()),
                    },
                ),
            })
            .expect_err("blank protocol submission identity must fail before dispatch");

        assert_eq!(error.code, "agent_core_required_field_missing");
        assert!(error.message.contains(field));
    }

    let mut invalid_trace = RuntimeTraceContext::for_run(
        &trace_id,
        "run-envelope-validation",
        "runtime_settings_change",
    );
    invalid_trace.span_id = "not-a-span".into();
    let error = runtime
        .submit_protocol(RuntimeSubmissionEnvelope {
            protocol_version: CORE_PROTOCOL_VERSION,
            submission_id: "submission-invalid-trace".into(),
            trace: invalid_trace,
            submitted_at: "2026-07-18T12:00:00Z".into(),
            submission: RuntimeSubmission::RuntimeSettingsChange(RuntimeSettingsChangeRequest {
                project_id: Some("project-1".into()),
                settings: serde_json::json!({}),
                reason: None,
            }),
        })
        .expect_err("malformed protocol trace must fail before dispatch");
    assert_eq!(error.code, "agent_protocol_trace_invalid");
}

#[test]
fn protocol_dispatches_every_stateful_fake_provider_submission_variant() {
    let runtime = FakeProviderRuntime::default();
    let project_id = "project-protocol-routing";
    let run_id = "run-protocol-routing";
    let trace_id = runtime_trace_id_for_run(project_id, run_id);
    let submit = |submission_id: &str, submission: RuntimeSubmission| {
        runtime.submit_protocol(RuntimeSubmissionEnvelope {
            protocol_version: CORE_PROTOCOL_VERSION,
            submission_id: submission_id.into(),
            trace: RuntimeTraceContext::for_run(&trace_id, run_id, submission_id),
            submitted_at: "2026-07-18T12:00:00Z".into(),
            submission,
        })
    };

    let started = submit(
        "submission-start",
        RuntimeSubmission::StartRun(StartRunRequest {
            project_id: project_id.into(),
            agent_session_id: "session-protocol-routing".into(),
            run_id: run_id.into(),
            prompt: "Start protocol routing fixture.".into(),
            provider: ProviderSelection {
                provider_id: "fake_provider".into(),
                model_id: "fake-model".into(),
            },
            controls: None,
        }),
    )
    .expect("dispatch start");
    assert!(started.snapshot.is_some());

    for (submission_id, submission) in [
        (
            "submission-continue",
            RuntimeSubmission::ContinueRun(ContinueRunRequest {
                project_id: project_id.into(),
                run_id: run_id.into(),
                prompt: "Continue through protocol.".into(),
            }),
        ),
        (
            "submission-user-message",
            RuntimeSubmission::UserMessage(UserInputRequest {
                project_id: project_id.into(),
                run_id: run_id.into(),
                text: "User message through protocol.".into(),
            }),
        ),
        (
            "submission-approval",
            RuntimeSubmission::ApprovalDecision(ApprovalDecisionRequest {
                project_id: project_id.into(),
                run_id: run_id.into(),
                action_id: "action-protocol".into(),
                response: Some("Approved through protocol.".into()),
            }),
        ),
        (
            "submission-cancel",
            RuntimeSubmission::Cancel(CancelRunRequest {
                project_id: project_id.into(),
                run_id: run_id.into(),
            }),
        ),
        (
            "submission-resume",
            RuntimeSubmission::Resume(ResumeRunRequest {
                project_id: project_id.into(),
                run_id: run_id.into(),
                response: "Resume through protocol.".into(),
            }),
        ),
    ] {
        let outcome = submit(submission_id, submission).expect("dispatch stateful submission");
        assert_eq!(outcome.accepted_submission_id, submission_id);
        assert!(outcome.snapshot.is_some());
        assert!(outcome.trace_export.is_none());
    }

    let grant = submit(
        "submission-tool-grant",
        RuntimeSubmission::ToolPermissionGrant(ToolPermissionGrantRequest {
            project_id: project_id.into(),
            run_id: run_id.into(),
            grant_id: "grant-1".into(),
            tool_name: "read".into(),
            expires_at: Some("2026-07-18T13:00:00Z".into()),
        }),
    )
    .expect("dispatch tool permission grant");
    assert_eq!(
        grant.snapshot.unwrap().events.last().unwrap().event_kind,
        RuntimeEventKind::ToolPermissionGrant
    );

    let provider_change = submit(
        "submission-provider-change",
        RuntimeSubmission::ProviderModelChange(ProviderModelChangeRequest {
            project_id: project_id.into(),
            run_id: Some(run_id.into()),
            provider: ProviderSelection {
                provider_id: "fake_provider".into(),
                model_id: "fake-model-next".into(),
            },
            reason: Some("fixture".into()),
        }),
    )
    .expect("dispatch run-scoped provider change");
    assert_eq!(
        provider_change
            .snapshot
            .unwrap()
            .events
            .last()
            .unwrap()
            .event_kind,
        RuntimeEventKind::ProviderModelChanged
    );

    for (submission_id, submission) in [
        (
            "submission-project-provider-change",
            RuntimeSubmission::ProviderModelChange(ProviderModelChangeRequest {
                project_id: project_id.into(),
                run_id: None,
                provider: ProviderSelection {
                    provider_id: "fake_provider".into(),
                    model_id: "fake-model-next".into(),
                },
                reason: None,
            }),
        ),
        (
            "submission-runtime-settings",
            RuntimeSubmission::RuntimeSettingsChange(RuntimeSettingsChangeRequest {
                project_id: Some(project_id.into()),
                settings: serde_json::json!({"approvalMode": "suggest"}),
                reason: Some("fixture".into()),
            }),
        ),
    ] {
        let outcome = submit(submission_id, submission).expect("dispatch non-run submission");
        assert!(outcome.snapshot.is_none());
        assert!(outcome.trace_export.is_none());
    }

    let exported = submit(
        "submission-export",
        RuntimeSubmission::ExportTrace(ExportTraceRequest {
            project_id: project_id.into(),
            run_id: run_id.into(),
        }),
    )
    .expect("dispatch trace export");
    assert!(exported.snapshot.is_none());
    assert_eq!(exported.trace_export.unwrap().snapshot.run_id, run_id);

    for (submission_id, submission, operation) in [
        (
            "submission-fork",
            RuntimeSubmission::Fork(ForkSessionRequest {
                project_id: project_id.into(),
                source_agent_session_id: "session-protocol-routing".into(),
                target_agent_session_id: "session-fork".into(),
            }),
            "fork_session",
        ),
        (
            "submission-compact",
            RuntimeSubmission::Compact(CompactSessionRequest {
                project_id: project_id.into(),
                agent_session_id: "session-protocol-routing".into(),
                reason: "fixture".into(),
            }),
            "compact_session",
        ),
    ] {
        let error = submit(submission_id, submission)
            .expect_err("unsupported fake-provider protocol operation");
        assert_eq!(error.code, "agent_core_operation_unsupported");
        assert!(error.message.contains(operation));
    }
}

#[test]
fn protocol_rejects_blank_tool_permission_grant_identifiers_without_an_event() {
    let runtime = FakeProviderRuntime::default();
    let project_id = "project-grant-validation";
    let run_id = "run-grant-validation";
    let before = runtime
        .start_run(StartRunRequest {
            project_id: project_id.into(),
            agent_session_id: "session-grant-validation".into(),
            run_id: run_id.into(),
            prompt: "Start grant validation fixture.".into(),
            provider: ProviderSelection {
                provider_id: "fake_provider".into(),
                model_id: "fake-model".into(),
            },
            controls: None,
        })
        .expect("start grant validation fixture");
    let trace_id = before.trace_id.clone();

    for (project_id, run_id, grant_id, tool_name, field) in [
        (" ", run_id, "grant-1", "read", "projectId"),
        (project_id, "\n", "grant-1", "read", "runId"),
        (project_id, run_id, "\t", "read", "grantId"),
        (project_id, run_id, "grant-1", " ", "toolName"),
    ] {
        let error = runtime
            .submit_protocol(RuntimeSubmissionEnvelope {
                protocol_version: CORE_PROTOCOL_VERSION,
                submission_id: format!("submission-invalid-{field}"),
                trace: RuntimeTraceContext::for_run(&trace_id, run_id, "tool_permission_grant"),
                submitted_at: "2026-07-18T12:00:00Z".into(),
                submission: RuntimeSubmission::ToolPermissionGrant(ToolPermissionGrantRequest {
                    project_id: project_id.into(),
                    run_id: run_id.into(),
                    grant_id: grant_id.into(),
                    tool_name: tool_name.into(),
                    expires_at: None,
                }),
            })
            .expect_err("blank grant identifier must fail before dispatch");
        assert_eq!(error.code, "agent_core_required_field_missing");
        assert!(error.message.contains(field));
    }

    assert_eq!(
        runtime
            .store()
            .load_run("project-grant-validation", "run-grant-validation")
            .expect("reload grant fixture")
            .events,
        before.events
    );
}

#[test]
fn protocol_rejects_blank_provider_change_identifiers_without_an_event() {
    let runtime = FakeProviderRuntime::default();
    let project_id = "project-provider-change-validation";
    let run_id = "run-provider-change-validation";
    let before = runtime
        .start_run(StartRunRequest {
            project_id: project_id.into(),
            agent_session_id: "session-provider-change-validation".into(),
            run_id: run_id.into(),
            prompt: "Start provider-change validation fixture.".into(),
            provider: ProviderSelection {
                provider_id: "fake_provider".into(),
                model_id: "fake-model".into(),
            },
            controls: None,
        })
        .expect("start provider-change validation fixture");

    for (project_id, scoped_run_id, provider_id, model_id, field) in [
        (
            " ",
            Some(run_id),
            "fake_provider",
            "fake-model-2",
            "projectId",
        ),
        (
            project_id,
            Some("\n"),
            "fake_provider",
            "fake-model-2",
            "runId",
        ),
        (project_id, Some(run_id), "\t", "fake-model-2", "providerId"),
        (project_id, Some(run_id), "fake_provider", " ", "modelId"),
    ] {
        let error = runtime
            .submit_protocol(RuntimeSubmissionEnvelope {
                protocol_version: CORE_PROTOCOL_VERSION,
                submission_id: format!("submission-invalid-provider-change-{field}"),
                trace: RuntimeTraceContext::for_run(
                    &before.trace_id,
                    run_id,
                    "provider_model_change",
                ),
                submitted_at: "2026-07-18T12:00:00Z".into(),
                submission: RuntimeSubmission::ProviderModelChange(ProviderModelChangeRequest {
                    project_id: project_id.into(),
                    run_id: scoped_run_id.map(str::to_owned),
                    provider: ProviderSelection {
                        provider_id: provider_id.into(),
                        model_id: model_id.into(),
                    },
                    reason: None,
                }),
            })
            .expect_err("blank provider-change identifier must fail before dispatch");
        assert_eq!(error.code, "agent_core_required_field_missing");
        assert!(error.message.contains(field));
    }

    assert_eq!(
        runtime
            .store()
            .load_run(project_id, run_id)
            .expect("reload provider-change fixture")
            .events,
        before.events
    );
}

#[test]
fn protocol_rejects_a_blank_optional_runtime_settings_project() {
    let runtime = FakeProviderRuntime::default();
    let trace_id = runtime_trace_id_for_run("project-settings", "run-settings");
    let error = runtime
        .submit_protocol(RuntimeSubmissionEnvelope {
            protocol_version: CORE_PROTOCOL_VERSION,
            submission_id: "submission-invalid-settings-project".into(),
            trace: RuntimeTraceContext::for_run(
                &trace_id,
                "run-settings",
                "runtime_settings_change",
            ),
            submitted_at: "2026-07-18T12:00:00Z".into(),
            submission: RuntimeSubmission::RuntimeSettingsChange(RuntimeSettingsChangeRequest {
                project_id: Some(" \n ".into()),
                settings: serde_json::json!({"approvalMode": "suggest"}),
                reason: None,
            }),
        })
        .expect_err("a present runtime-settings project id cannot be blank");

    assert_eq!(error.code, "agent_core_required_field_missing");
    assert!(error.message.contains("projectId"));
}

#[test]
fn reusable_facades_reject_blank_action_ids_without_mutating_the_run() {
    let direct = FakeProviderRuntime::default();
    let direct_before = direct
        .start_run(StartRunRequest {
            project_id: "project-direct-action-validation".into(),
            agent_session_id: "session-direct-action-validation".into(),
            run_id: "run-direct-action-validation".into(),
            prompt: "Start direct action validation fixture.".into(),
            provider: ProviderSelection {
                provider_id: "fake_provider".into(),
                model_id: "fake-model".into(),
            },
            controls: None,
        })
        .expect("start direct action fixture");
    for approve in [true, false] {
        let request = ApprovalDecisionRequest {
            project_id: direct_before.project_id.clone(),
            run_id: direct_before.run_id.clone(),
            action_id: " \n ".into(),
            response: Some("fixture response".into()),
        };
        let error = if approve {
            direct.approve_action(request)
        } else {
            direct.reject_action(request)
        }
        .expect_err("direct facade must reject a blank action id");
        assert_eq!(error.code, "agent_core_required_field_missing");
        assert!(error.message.contains("actionId"));
    }
    assert_eq!(
        direct
            .store()
            .load_run(&direct_before.project_id, &direct_before.run_id)
            .expect("reload direct action fixture"),
        direct_before
    );

    let headless = HeadlessProviderRuntime::new(
        InMemoryAgentCoreStore::default(),
        HeadlessProviderExecutionConfig::Fake,
        HeadlessRuntimeOptions::default(),
    );
    let headless_before = headless
        .start_run(StartRunRequest {
            project_id: "project-headless-action-validation".into(),
            agent_session_id: "session-headless-action-validation".into(),
            run_id: "run-headless-action-validation".into(),
            prompt: "Start headless action validation fixture.".into(),
            provider: ProviderSelection {
                provider_id: "fake_provider".into(),
                model_id: "fake-model".into(),
            },
            controls: None,
        })
        .expect("start headless action fixture");
    for approve in [true, false] {
        let request = ApprovalDecisionRequest {
            project_id: headless_before.project_id.clone(),
            run_id: headless_before.run_id.clone(),
            action_id: "\t".into(),
            response: Some("fixture response".into()),
        };
        let error = if approve {
            headless.approve_action(request)
        } else {
            headless.reject_action(request)
        }
        .expect_err("headless facade must reject a blank action id");
        assert_eq!(error.code, "agent_core_required_field_missing");
        assert!(error.message.contains("actionId"));
    }
    assert_eq!(
        headless
            .store()
            .load_run(&headless_before.project_id, &headless_before.run_id)
            .expect("reload headless action fixture"),
        headless_before
    );
}

#[test]
fn blocked_real_provider_continuation_does_not_mutate_the_persisted_run() {
    let store = InMemoryAgentCoreStore::default();
    let before = store
        .insert_run(NewRunRecord {
            trace_id: None,
            runtime_agent_id: "engineer".into(),
            agent_definition_id: "engineer".into(),
            agent_definition_version: 1,
            system_prompt: "Fixture system prompt.".into(),
            project_id: "project-blocked-continuation".into(),
            agent_session_id: "session-blocked-continuation".into(),
            run_id: "run-blocked-continuation".into(),
            provider_id: "openai_api".into(),
            model_id: "test-model".into(),
            prompt: "Start blocked continuation fixture.".into(),
        })
        .expect("insert blocked continuation fixture");
    let blocked_preflight = provider_preflight_snapshot(ProviderPreflightInput {
        profile_id: "openai_api".into(),
        provider_id: "openai_api".into(),
        model_id: "test-model".into(),
        source: ProviderPreflightSource::LiveProbe,
        checked_at: "2026-07-18T12:00:00Z".into(),
        age_seconds: Some(0),
        ttl_seconds: None,
        required_features: ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
        capabilities: provider_capability_catalog(ProviderCapabilityCatalogInput {
            provider_id: "openai_api".into(),
            model_id: "test-model".into(),
            catalog_source: "fixture".into(),
            fetched_at: Some("2026-07-18T12:00:00Z".into()),
            last_success_at: None,
            cache_age_seconds: Some(0),
            cache_ttl_seconds: None,
            credential_proof: None,
            context_window_tokens: Some(128_000),
            max_output_tokens: Some(16_384),
            context_limit_source: Some("fixture".into()),
            context_limit_confidence: Some("high".into()),
            thinking_supported: false,
            thinking_efforts: Vec::new(),
            thinking_default_effort: None,
            input_modalities: Vec::new(),
            input_modalities_source: Some("unknown".into()),
        }),
        credential_ready: Some(false),
        endpoint_reachable: Some(true),
        model_available: Some(true),
        streaming_route_available: Some(true),
        tool_schema_accepted: Some(true),
        reasoning_controls_accepted: None,
        attachments_accepted: None,
        context_limit_known: Some(true),
        provider_error: None,
    });
    let runtime = HeadlessProviderRuntime::new(
        store.clone(),
        HeadlessProviderExecutionConfig::OpenAiCompatible(OpenAiCompatibleHeadlessConfig {
            provider_id: "openai_api".into(),
            model_id: "test-model".into(),
            base_url: "http://127.0.0.1:9/v1".into(),
            api_key: None,
            timeout_ms: 1,
            workspace_root: None,
            allow_workspace_writes: false,
        }),
        HeadlessRuntimeOptions {
            provider_preflight: Some(blocked_preflight),
            ..HeadlessRuntimeOptions::default()
        },
    );

    let error = runtime
        .continue_run(ContinueRunRequest {
            project_id: before.project_id.clone(),
            run_id: before.run_id.clone(),
            prompt: "This must not be persisted while preflight is blocked.".into(),
        })
        .expect_err("blocked continuation preflight must fail");

    assert_eq!(error.code, "agent_core_provider_preflight_blocked");
    assert_eq!(
        store
            .load_run(&before.project_id, &before.run_id)
            .expect("reload blocked continuation fixture"),
        before,
        "preflight admission must happen before status, message, or event writes"
    );
}

#[test]
fn headless_fake_facade_covers_turn_control_and_session_operations() {
    let runtime = HeadlessProviderRuntime::new(
        InMemoryAgentCoreStore::default(),
        HeadlessProviderExecutionConfig::Fake,
        HeadlessRuntimeOptions::default(),
    );
    let started = runtime
        .start_run(StartRunRequest {
            project_id: "project-headless-facade".into(),
            agent_session_id: "session-source".into(),
            run_id: "run-headless-facade".into(),
            prompt: "Start the headless facade fixture.".into(),
            provider: ProviderSelection {
                provider_id: "fake_provider".into(),
                model_id: "fake-model".into(),
            },
            controls: None,
        })
        .expect("start facade fixture");
    assert_eq!(started.status, RunStatus::Completed);

    let continued = runtime
        .continue_run(ContinueRunRequest {
            project_id: started.project_id.clone(),
            run_id: started.run_id.clone(),
            prompt: "Continue through the facade.".into(),
        })
        .expect("continue facade fixture");
    assert_eq!(continued.context_manifests.len(), 2);

    runtime
        .submit_user_input(UserInputRequest {
            project_id: started.project_id.clone(),
            run_id: started.run_id.clone(),
            text: "Submit user input through the facade.".into(),
        })
        .expect("submit user input");
    runtime
        .approve_action(ApprovalDecisionRequest {
            project_id: started.project_id.clone(),
            run_id: started.run_id.clone(),
            action_id: "action-approve".into(),
            response: None,
        })
        .expect("approve action");
    let rejected = runtime
        .reject_action(ApprovalDecisionRequest {
            project_id: started.project_id.clone(),
            run_id: started.run_id.clone(),
            action_id: "action-reject".into(),
            response: Some("No".into()),
        })
        .expect("reject action");
    assert_eq!(
        rejected.events.last().unwrap().event_kind,
        RuntimeEventKind::PolicyDecision
    );

    assert_eq!(
        runtime
            .cancel_run(CancelRunRequest {
                project_id: started.project_id.clone(),
                run_id: started.run_id.clone(),
            })
            .expect("cancel run")
            .status,
        RunStatus::Cancelled
    );
    assert_eq!(
        runtime
            .resume_run(ResumeRunRequest {
                project_id: started.project_id.clone(),
                run_id: started.run_id.clone(),
                response: "Resume after cancellation.".into(),
            })
            .expect("resume run")
            .status,
        RunStatus::Completed
    );

    let compacted = runtime
        .compact_session(CompactSessionRequest {
            project_id: started.project_id.clone(),
            agent_session_id: started.agent_session_id.clone(),
            reason: "fixture compaction".into(),
        })
        .expect("compact source session");
    assert!(
        compacted.context_manifests.last().unwrap().manifest["summary"]
            .as_str()
            .is_some_and(|summary| summary.contains("Resume after cancellation."))
    );

    let forked = runtime
        .fork_session(ForkSessionRequest {
            project_id: started.project_id.clone(),
            source_agent_session_id: started.agent_session_id.clone(),
            target_agent_session_id: "session-forked".into(),
        })
        .expect("fork source session");
    assert_eq!(forked.agent_session_id, "session-forked");
    assert_eq!(forked.status, RunStatus::Completed);
    assert_eq!(forked.messages.len(), compacted.messages.len());
    assert!(forked
        .messages
        .iter()
        .zip(&compacted.messages)
        .all(|(forked, source)| forked.role == source.role && forked.content == source.content));

    let trace = runtime
        .export_trace(ExportTraceRequest {
            project_id: forked.project_id.clone(),
            run_id: forked.run_id.clone(),
        })
        .expect("export forked trace");
    assert_eq!(trace.snapshot, forked);
}
