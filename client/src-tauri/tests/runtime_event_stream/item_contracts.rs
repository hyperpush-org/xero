use super::support::*;

pub(crate) fn runtime_stream_appends_pending_action_required_after_replay_with_monotonic_sequence()
{
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let runtime = seed_authenticated_runtime(&app, &auth_store_path, &project_id);

    let launched = launch_supervised_run(
        app.state::<DesktopState>().inner(),
        &project_id,
        &repo_root,
        "run-action-required",
        &runtime_shell::script_print_line_and_sleep("backlog ready", 3),
    );

    wait_for_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        |snapshot| {
            snapshot.run.status == RuntimeRunStatus::Running
                && snapshot.last_checkpoint_sequence >= 1
        },
    );
    let action_id = seed_pending_operator_approval(
        &repo_root,
        &project_id,
        &launched.run.run_id,
        runtime.session_id.as_deref().expect("session id"),
    );

    let (channel, receiver) = capture_stream_channel();
    start_direct_runtime_stream(
        &app,
        &project_id,
        &repo_root,
        &runtime,
        &launched.run.run_id,
        vec![
            RuntimeStreamItemKind::Transcript,
            RuntimeStreamItemKind::ActionRequired,
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
            RuntimeStreamItemKind::Transcript,
            RuntimeStreamItemKind::ActionRequired,
            RuntimeStreamItemKind::Complete,
        ]
    );
    assert_eq!(items[0].text.as_deref(), Some("backlog ready"));
    assert_eq!(items[1].action_id.as_deref(), Some(action_id.as_str()));
    assert_eq!(items[1].boundary_id.as_deref(), Some("boundary-pending"));
    assert_eq!(
        items[1].action_type.as_deref(),
        Some("terminal_input_required")
    );
    assert_eq!(items[1].title.as_deref(), Some("Terminal input required"));
    assert_eq!(
        items[1].detail.as_deref(),
        Some("Provide terminal input to continue this run.")
    );

    stop_supervisor_run(app.state::<DesktopState>().inner(), &project_id, &repo_root);
}

pub(crate) fn runtime_stream_dedupes_replayed_action_required_against_durable_pending_queue() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let runtime = seed_authenticated_runtime(&app, &auth_store_path, &project_id);

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind fake supervisor listener");
    let endpoint = listener
        .local_addr()
        .expect("read fake supervisor endpoint")
        .to_string();

    seed_fake_runtime_run(
        &repo_root,
        &project_id,
        "run-dedupe-action-required",
        &endpoint,
    );

    let action_id = seed_runtime_action_required(
        &repo_root,
        &project_id,
        "run-dedupe-action-required",
        runtime.session_id.as_deref().expect("session id"),
        "boundary-1",
        "2026-04-15T23:10:02Z",
    );
    seed_blocked_autonomous_run(
        &repo_root,
        &project_id,
        "run-dedupe-action-required",
        &action_id,
        "boundary-1",
    );

    let server = thread::spawn({
        let project_id = project_id.clone();
        let action_id = action_id.clone();
        move || {
            let (mut stream, _) = listener.accept().expect("accept fake supervisor attach");
            let mut line = String::new();
            BufReader::new(stream.try_clone().expect("clone fake supervisor stream"))
                .read_line(&mut line)
                .expect("read attach request");
            let request: SupervisorControlRequest =
                serde_json::from_str(line.trim()).expect("decode attach request");
            assert!(matches!(
                request,
                SupervisorControlRequest::Attach {
                    project_id: requested_project_id,
                    run_id,
                    after_sequence: None,
                    ..
                } if requested_project_id == project_id && run_id == "run-dedupe-action-required"
            ));

            write_json_line(
                &mut stream,
                &SupervisorControlResponse::Attached {
                    protocol_version: SUPERVISOR_PROTOCOL_VERSION,
                    project_id: project_id.clone(),
                    run_id: "run-dedupe-action-required".into(),
                    after_sequence: None,
                    replayed_count: 1,
                    replay_truncated: false,
                    oldest_available_sequence: Some(1),
                    latest_sequence: Some(1),
                },
            );
            write_json_line(
                &mut stream,
                &SupervisorControlResponse::Event {
                    protocol_version: SUPERVISOR_PROTOCOL_VERSION,
                    project_id,
                    run_id: "run-dedupe-action-required".into(),
                    sequence: 1,
                    created_at: "2026-04-15T23:10:02Z".into(),
                    replay: true,
                    item: SupervisorLiveEventPayload::ActionRequired {
                        action_id,
                        boundary_id: "boundary-1".into(),
                        action_type: "terminal_input_required".into(),
                        title: "Terminal input required".into(),
                        detail: "Provide terminal input to continue this run.".into(),
                    },
                },
            );
            thread::sleep(Duration::from_millis(150));
        }
    });

    let (channel, receiver) = capture_stream_channel();
    start_direct_runtime_stream(
        &app,
        &project_id,
        &repo_root,
        &runtime,
        "run-dedupe-action-required",
        vec![
            RuntimeStreamItemKind::ActionRequired,
            RuntimeStreamItemKind::Failure,
        ],
        channel,
    );

    let items = collect_until_terminal(receiver);
    let action_required_items = items
        .iter()
        .filter(|item| item.kind == RuntimeStreamItemKind::ActionRequired)
        .collect::<Vec<_>>();

    assert_eq!(
        action_required_items.len(),
        1,
        "expected one deduped action-required item, got {items:?}"
    );
    assert_eq!(
        action_required_items[0].action_id.as_deref(),
        Some(action_id.as_str())
    );
    assert_eq!(
        action_required_items[0].boundary_id.as_deref(),
        Some("boundary-1")
    );

    let autonomous_snapshot = project_store::load_autonomous_run(&repo_root, &project_id)
        .expect("load autonomous run after replayed runtime stream")
        .expect("autonomous run should still exist after replayed runtime stream");
    let blocked_evidence = autonomous_snapshot
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .filter(|artifact| {
            matches!(
                artifact.payload.as_ref(),
                Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                    if payload.action_id.as_deref() == Some(action_id.as_str())
                        && payload.boundary_id.as_deref() == Some("boundary-1")
                        && payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Blocked
            )
        })
        .count();
    assert_eq!(blocked_evidence, 1);

    server.join().expect("join fake supervisor thread");
}

pub(crate) fn runtime_stream_redacts_secret_bearing_replay_without_leaking_tokens() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let runtime = seed_authenticated_runtime(&app, &auth_store_path, &project_id);

    let access_token = jwt_with_account_id("acct-1");
    let live_lines = vec![
        "access_token=shh-secret-value".to_string(),
        format!(
            "{STRUCTURED_EVENT_PREFIX}{{\"kind\":\"activity\",\"code\":\"diag\",\"title\":\"Auth\",\"detail\":\"Bearer hidden-token\"}}"
        ),
    ];

    let launched = launch_supervised_run(
        app.state::<DesktopState>().inner(),
        &project_id,
        &repo_root,
        "run-redaction-bridge",
        &runtime_shell::script_print_lines_and_sleep(&live_lines, 3),
    );

    wait_for_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        |snapshot| {
            snapshot.run.status == RuntimeRunStatus::Running
                && snapshot.last_checkpoint_sequence >= 2
        },
    );

    let (channel, receiver) = capture_stream_channel();
    start_direct_runtime_stream(
        &app,
        &project_id,
        &repo_root,
        &runtime,
        &launched.run.run_id,
        vec![
            RuntimeStreamItemKind::Activity,
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
            RuntimeStreamItemKind::Activity,
            RuntimeStreamItemKind::Activity,
            RuntimeStreamItemKind::Complete,
        ]
    );
    assert!(items[..2].iter().all(|item| {
        item.code.as_deref() == Some("runtime_supervisor_live_event_redacted")
            && item.title.as_deref() == Some("Live output redacted")
    }));

    let serialized_items = serde_json::to_string(&items).expect("serialize bridged items");
    assert!(!serialized_items.contains("access_token"));
    assert!(!serialized_items.contains("Bearer"));
    assert!(!serialized_items.contains("sk-"));
    assert!(!serialized_items.contains("refresh-1"));
    assert!(!serialized_items.contains(&access_token));

    let persisted = project_store::load_runtime_run(&repo_root, &project_id)
        .expect("load stored runtime run")
        .expect("stored runtime run should exist");
    let checkpoint_dump = persisted
        .checkpoints
        .iter()
        .map(|checkpoint| checkpoint.summary.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!checkpoint_dump.contains("access_token"));
    assert!(!checkpoint_dump.contains("Bearer"));
    assert!(!checkpoint_dump.contains("refresh-1"));
    assert!(!checkpoint_dump.contains(&access_token));
    assert!(checkpoint_dump.contains("runtime_supervisor_live_event_redacted"));

    stop_supervisor_run(app.state::<DesktopState>().inner(), &project_id, &repo_root);
}

pub(crate) fn runtime_stream_replays_mcp_tool_summary_variant_with_monotonic_sequence() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let runtime = seed_authenticated_runtime(&app, &auth_store_path, &project_id);

    let live_lines = vec![
        format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "tool",
                "tool_call_id": "tool-mcp-1",
                "tool_name": "mcp.invoke",
                "tool_state": "running",
                "detail": "Invoking MCP resource",
                "tool_summary": {
                    "kind": "mcp_capability",
                    "serverId": "workspace-mcp",
                    "capabilityKind": "resource",
                    "capabilityId": "file://README.md",
                    "capabilityName": "README"
                }
            })
        ),
        format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "tool",
                "tool_call_id": "tool-file-1",
                "tool_name": "read",
                "tool_state": "succeeded",
                "detail": "Read repository overview",
                "tool_summary": {
                    "kind": "file",
                    "path": "README.md",
                    "scope": null,
                    "lineCount": 64,
                    "matchCount": null,
                    "truncated": false
                }
            })
        ),
    ];

    let launched = launch_supervised_run(
        app.state::<DesktopState>().inner(),
        &project_id,
        &repo_root,
        "run-mcp-stream",
        &runtime_shell::script_print_lines_and_sleep(&live_lines, 3),
    );

    wait_for_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        |snapshot| {
            snapshot.run.status == RuntimeRunStatus::Running
                && snapshot.last_checkpoint_sequence >= 2
        },
    );

    let (channel, receiver) = capture_stream_channel();
    start_direct_runtime_stream(
        &app,
        &project_id,
        &repo_root,
        &runtime,
        &launched.run.run_id,
        vec![RuntimeStreamItemKind::Tool, RuntimeStreamItemKind::Complete],
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
            RuntimeStreamItemKind::Complete,
        ]
    );

    assert!(matches!(
        &items[0],
        RuntimeStreamItemDto {
            kind: RuntimeStreamItemKind::Tool,
            tool_call_id: Some(tool_call_id),
            tool_name: Some(tool_name),
            tool_state: Some(RuntimeToolCallState::Running),
            detail: Some(detail),
            tool_summary: Some(ToolResultSummaryDto::McpCapability(summary)),
            ..
        } if tool_call_id == "tool-mcp-1"
            && tool_name == "mcp.invoke"
            && detail == "Invoking MCP resource"
            && summary.server_id == "workspace-mcp"
            && summary.capability_kind == cadence_desktop_lib::commands::McpCapabilityKindDto::Resource
            && summary.capability_id == "file://README.md"
            && summary.capability_name.as_deref() == Some("README")
    ));

    assert!(matches!(
        &items[1],
        RuntimeStreamItemDto {
            kind: RuntimeStreamItemKind::Tool,
            tool_call_id: Some(tool_call_id),
            tool_name: Some(tool_name),
            tool_state: Some(RuntimeToolCallState::Succeeded),
            detail: Some(detail),
            tool_summary: Some(ToolResultSummaryDto::File(summary)),
            ..
        } if tool_call_id == "tool-file-1"
            && tool_name == "read"
            && detail == "Read repository overview"
            && summary.path.as_deref() == Some("README.md")
            && summary.line_count == Some(64)
    ));

    stop_supervisor_run(app.state::<DesktopState>().inner(), &project_id, &repo_root);
}

pub(crate) fn runtime_stream_replays_browser_computer_use_tool_summary_variant_with_monotonic_sequence(
) {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let runtime = seed_authenticated_runtime(&app, &auth_store_path, &project_id);

    let live_lines = vec![
        format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "tool",
                "tool_call_id": "tool-browser-1",
                "tool_name": "browser.click",
                "tool_state": "running",
                "detail": "Clicking primary action",
                "tool_summary": {
                    "kind": "browser_computer_use",
                    "surface": "browser",
                    "action": "click",
                    "status": "running",
                    "target": "button#primary",
                    "outcome": null
                }
            })
        ),
        format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "tool",
                "tool_call_id": "tool-browser-2",
                "tool_name": "browser.navigate",
                "tool_state": "succeeded",
                "detail": "Navigation completed",
                "tool_summary": {
                    "kind": "browser_computer_use",
                    "surface": "browser",
                    "action": "navigate",
                    "status": "succeeded",
                    "target": "https://example.com/docs",
                    "outcome": "Loaded docs page"
                }
            })
        ),
        format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "tool",
                "tool_call_id": "tool-computer-1",
                "tool_name": "computer.drag",
                "tool_state": "failed",
                "detail": "Desktop drag blocked",
                "tool_summary": {
                    "kind": "browser_computer_use",
                    "surface": "computer_use",
                    "action": "drag",
                    "status": "blocked",
                    "target": "Desktop icon",
                    "outcome": "Permission denied"
                }
            })
        ),
    ];

    let launched = launch_supervised_run(
        app.state::<DesktopState>().inner(),
        &project_id,
        &repo_root,
        "run-browser-stream",
        &runtime_shell::script_print_lines_and_sleep(&live_lines, 3),
    );

    wait_for_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        |snapshot| {
            snapshot.run.status == RuntimeRunStatus::Running
                && snapshot.last_checkpoint_sequence >= 3
        },
    );

    let (channel, receiver) = capture_stream_channel();
    start_direct_runtime_stream(
        &app,
        &project_id,
        &repo_root,
        &runtime,
        &launched.run.run_id,
        vec![RuntimeStreamItemKind::Tool, RuntimeStreamItemKind::Complete],
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
            RuntimeStreamItemKind::Tool,
            RuntimeStreamItemKind::Complete,
        ]
    );

    assert!(matches!(
        &items[0],
        RuntimeStreamItemDto {
            kind: RuntimeStreamItemKind::Tool,
            tool_call_id: Some(tool_call_id),
            tool_name: Some(tool_name),
            tool_state: Some(RuntimeToolCallState::Running),
            detail: Some(detail),
            tool_summary: Some(ToolResultSummaryDto::BrowserComputerUse(summary)),
            ..
        } if tool_call_id == "tool-browser-1"
            && tool_name == "browser.click"
            && detail == "Clicking primary action"
            && summary.surface == cadence_desktop_lib::commands::BrowserComputerUseSurfaceDto::Browser
            && summary.action == "click"
            && summary.status == cadence_desktop_lib::commands::BrowserComputerUseActionStatusDto::Running
            && summary.target.as_deref() == Some("button#primary")
            && summary.outcome.is_none()
    ));

    assert!(matches!(
        &items[2],
        RuntimeStreamItemDto {
            kind: RuntimeStreamItemKind::Tool,
            tool_call_id: Some(tool_call_id),
            tool_name: Some(tool_name),
            tool_state: Some(RuntimeToolCallState::Failed),
            detail: Some(detail),
            tool_summary: Some(ToolResultSummaryDto::BrowserComputerUse(summary)),
            ..
        } if tool_call_id == "tool-computer-1"
            && tool_name == "computer.drag"
            && detail == "Desktop drag blocked"
            && summary.surface == cadence_desktop_lib::commands::BrowserComputerUseSurfaceDto::ComputerUse
            && summary.action == "drag"
            && summary.status == cadence_desktop_lib::commands::BrowserComputerUseActionStatusDto::Blocked
            && summary.target.as_deref() == Some("Desktop icon")
            && summary.outcome.as_deref() == Some("Permission denied")
    ));

    stop_supervisor_run(app.state::<DesktopState>().inner(), &project_id, &repo_root);
}

pub(crate) fn runtime_stream_contract_serialization_exposes_run_id_sequence_and_activity() {
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
        created_at: "2026-04-15T23:10:02Z".into(),
    })
    .expect("serialize runtime stream activity item");
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
            "createdAt": "2026-04-15T23:10:02Z"
        })
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
            "failure",
        ]
    );

    let project_updated = serde_json::to_value(ProjectUpdatedPayloadDto {
        project: ProjectSummaryDto {
            id: "project-1".into(),
            name: "Cadence".into(),
            description: "Desktop shell".into(),
            milestone: "M004".into(),
            total_phases: 4,
            completed_phases: 2,
            active_phase: 3,
            branch: Some("main".into()),
            runtime: Some("openai_codex".into()),
        },
        reason: ProjectUpdateReason::MetadataChanged,
    })
    .expect("serialize project updated payload");

    assert_eq!(project_updated["project"]["activePhase"], json!(3));
    assert_eq!(project_updated["project"]["completedPhases"], json!(2));
    assert_eq!(project_updated["reason"], json!("metadata_changed"));

    let checkpoint = serde_json::to_value(cadence_desktop_lib::commands::RuntimeRunCheckpointDto {
        sequence: 3,
        kind: RuntimeRunCheckpointKindDto::ActionRequired,
        summary: "Approval required".into(),
        created_at: "2026-04-15T23:10:03Z".into(),
    })
    .expect("serialize runtime checkpoint");
    assert_eq!(checkpoint["sequence"], json!(3));

    let transport = serde_json::to_value(cadence_desktop_lib::commands::RuntimeRunTransportDto {
        kind: "tcp".into(),
        endpoint: "127.0.0.1:45123".into(),
        liveness: RuntimeRunTransportLivenessDto::Reachable,
    })
    .expect("serialize runtime transport");
    assert_eq!(transport["liveness"], json!("reachable"));
}
