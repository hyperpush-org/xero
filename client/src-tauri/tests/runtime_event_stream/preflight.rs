use super::support::*;

pub(crate) fn subscribe_runtime_stream_rejects_missing_channel_and_unsupported_kind_lists_activity()
{
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("failed to create mock webview window");

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            SUBSCRIBE_RUNTIME_STREAM_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1",
                    "agentSessionId": "agent-session-main",
                    "itemKinds": ["transcript"]
                }
            }),
        ),
        Err(CommandError::user_fixable(
            "runtime_stream_channel_missing",
            "Cadence requires a runtime stream channel before it can start streaming selected-project runtime items.",
        )),
    );

    tauri::test::assert_ipc_response(
        &webview,
        subscribe_request("project-1", &channel_string(), &["bogus"]),
        Err(CommandError::user_fixable(
            "runtime_stream_item_kind_unsupported",
            "Cadence does not support runtime stream item kind `bogus`. Allowed kinds: transcript, tool, skill, activity, action_required, complete, failure.",
        )),
    );
}

pub(crate) fn subscribe_runtime_stream_fails_closed_without_an_attachable_durable_run() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("failed to create mock webview window");

    seed_authenticated_runtime(&app, &auth_store_path, &project_id);

    tauri::test::assert_ipc_response(
        &webview,
        subscribe_request(&project_id, &channel_string(), &["transcript", "failure"]),
        Err(CommandError {
            code: "runtime_stream_run_unavailable".into(),
            class: CommandErrorClass::Retryable,
            message: "Cadence cannot start a live runtime stream until the selected project has an attachable durable run.".into(),
            retryable: true,
        }),
    );

    seed_terminal_runtime_run(&repo_root, &project_id, "run-failed");

    tauri::test::assert_ipc_response(
        &webview,
        subscribe_request(&project_id, &channel_string(), &["transcript", "failure"]),
        Err(CommandError {
            code: "runtime_stream_run_unavailable".into(),
            class: CommandErrorClass::UserFixable,
            message: "The detached runtime supervisor exited with status 17.".into(),
            retryable: false,
        }),
    );
}

pub(crate) fn subscribe_runtime_stream_returns_run_scoped_response_for_an_attachable_run() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("failed to create mock webview window");

    let seeded_runtime = seed_authenticated_runtime(&app, &auth_store_path, &project_id);
    let launched = launch_supervised_run(
        app.state::<DesktopState>().inner(),
        &project_id,
        &repo_root,
        "run-subscribe",
        &runtime_shell::script_print_line_and_sleep("ready", 2),
    );

    tauri::test::assert_ipc_response(
        &webview,
        subscribe_request(
            &project_id,
            &channel_string(),
            &["transcript", "tool", "skill", "activity", "complete"],
        ),
        Ok(SubscribeRuntimeStreamResponseDto {
            project_id: project_id.clone(),
            agent_session_id: "agent-session-main".into(),
            runtime_kind: seeded_runtime.runtime_kind.clone(),
            run_id: launched.run.run_id.clone(),
            session_id: seeded_runtime
                .session_id
                .clone()
                .expect("authenticated runtime session id should exist"),
            flow_id: seeded_runtime.flow_id.clone(),
            subscribed_item_kinds: vec![
                RuntimeStreamItemKind::Transcript,
                RuntimeStreamItemKind::Tool,
                RuntimeStreamItemKind::Skill,
                RuntimeStreamItemKind::Activity,
                RuntimeStreamItemKind::Complete,
            ],
        }),
    );

    stop_supervisor_run(app.state::<DesktopState>().inner(), &project_id, &repo_root);
}
