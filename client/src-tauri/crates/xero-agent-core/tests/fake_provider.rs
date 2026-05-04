use xero_agent_core::{
    runtime_trace_id_for_run, AgentProtocolRuntime, AgentRuntimeFacade, FakeProviderRuntime,
    ProviderSelection, RunControls, RunStatus, RuntimeEventKind, RuntimeSubmission,
    RuntimeSubmissionEnvelope, RuntimeTraceContext, StartRunRequest, CORE_PROTOCOL_VERSION,
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
