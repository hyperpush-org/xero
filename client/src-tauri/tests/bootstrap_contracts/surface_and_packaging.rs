use super::support::*;
use tauri::Manager;

pub(crate) fn builder_boots_and_registered_commands_return_expected_contract_shapes() {
    let (app, _temp_root) = build_mock_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("failed to create mock webview window");

    assert_eq!(
        REGISTERED_COMMAND_NAMES.len(),
        52,
        "expected fifty-two desktop commands"
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
            LIST_PROJECT_FILES_COMMAND,
            json!({ "request": { "projectId": "project-1" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            READ_PROJECT_FILE_COMMAND,
            json!({ "request": { "projectId": "project-1", "path": "/README.md" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            WRITE_PROJECT_FILE_COMMAND,
            json!({ "request": { "projectId": "project-1", "path": "/README.md", "content": "Cadence" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            CREATE_PROJECT_ENTRY_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1",
                    "parentPath": "/",
                    "name": "notes.md",
                    "entryType": "file"
                }
            }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            RENAME_PROJECT_ENTRY_COMMAND,
            json!({ "request": { "projectId": "project-1", "path": "/README.md", "newName": "README-2.md" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            DELETE_PROJECT_ENTRY_COMMAND,
            json!({ "request": { "projectId": "project-1", "path": "/README.md" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            GET_AUTONOMOUS_RUN_COMMAND,
            json!({ "request": { "projectId": "project-1", "agentSessionId": "agent-session-main" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            GET_RUNTIME_RUN_COMMAND,
            json!({ "request": { "projectId": "project-1", "agentSessionId": "agent-session-main" } }),
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
            anthropic_api_key_configured: false,
        }),
    );

    let expected_mcp_registry = cadence_desktop_lib::commands::list_mcp_servers(
        app.handle().clone(),
        app.state::<DesktopState>(),
    )
    .expect("list mcp servers should return default registry");

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(LIST_MCP_SERVERS_COMMAND, json!({})),
        Ok(expected_mcp_registry),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            UPSERT_MCP_SERVER_COMMAND,
            json!({
                "request": {
                    "id": "",
                    "name": "Bad",
                    "transport": { "kind": "stdio", "command": "node" }
                }
            }),
        ),
        Err(CommandError::invalid_request("id")),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            REMOVE_MCP_SERVER_COMMAND,
            json!({ "request": { "serverId": "" } }),
        ),
        Err(CommandError::invalid_request("serverId")),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            IMPORT_MCP_SERVERS_COMMAND,
            json!({ "request": { "path": "" } }),
        ),
        Err(CommandError::invalid_request("path")),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            REFRESH_MCP_SERVER_STATUSES_COMMAND,
            json!({ "request": { "serverIds": [""] } }),
        ),
        Err(CommandError::invalid_request("serverIds")),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            cadence_desktop_lib::commands::LIST_PROVIDER_PROFILES_COMMAND,
            json!({}),
        ),
        Ok(cadence_desktop_lib::commands::ProviderProfilesDto {
            active_profile_id: "openai_codex-default".into(),
            profiles: vec![cadence_desktop_lib::commands::ProviderProfileDto {
                profile_id: "openai_codex-default".into(),
                provider_id: "openai_codex".into(),
                runtime_kind: "openai_codex".into(),
                label: "OpenAI Codex".into(),
                model_id: "openai_codex".into(),
                preset_id: None,
                base_url: None,
                api_version: None,
                region: None,
                project_id: None,
                active: true,
                readiness: cadence_desktop_lib::commands::ProviderProfileReadinessDto {
                    ready: false,
                    status:
                        cadence_desktop_lib::commands::ProviderProfileReadinessStatusDto::Missing,
                    proof: None,
                    proof_updated_at: None,
                },
                migrated_from_legacy: false,
                migrated_at: None,
            }],
            migration: None,
        }),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            cadence_desktop_lib::commands::UPSERT_PROVIDER_PROFILE_COMMAND,
            json!({
                "request": {
                    "profileId": "zz-openai-alt",
                    "providerId": "openai_codex",
                    "runtimeKind": "openai_codex",
                    "label": "OpenAI Alt",
                    "modelId": "openai_codex",
                    "activate": false
                }
            }),
        ),
        Ok(cadence_desktop_lib::commands::ProviderProfilesDto {
            active_profile_id: "openai_codex-default".into(),
            profiles: vec![
                cadence_desktop_lib::commands::ProviderProfileDto {
                    profile_id: "openai_codex-default".into(),
                    provider_id: "openai_codex".into(),
                    runtime_kind: "openai_codex".into(),
                    label: "OpenAI Codex".into(),
                    model_id: "openai_codex".into(),
                    preset_id: None,
                    base_url: None,
                    api_version: None,
            region: None,
            project_id: None,
                    active: true,
                    readiness: cadence_desktop_lib::commands::ProviderProfileReadinessDto {
                        ready: false,
                        status: cadence_desktop_lib::commands::ProviderProfileReadinessStatusDto::Missing,
                        proof: None,
                        proof_updated_at: None,
                    },
                    migrated_from_legacy: false,
                    migrated_at: None,
                },
                cadence_desktop_lib::commands::ProviderProfileDto {
                    profile_id: "zz-openai-alt".into(),
                    provider_id: "openai_codex".into(),
                    runtime_kind: "openai_codex".into(),
                    label: "OpenAI Alt".into(),
                    model_id: "openai_codex".into(),
                    preset_id: None,
                    base_url: None,
                    api_version: None,
            region: None,
            project_id: None,
                    active: false,
                    readiness: cadence_desktop_lib::commands::ProviderProfileReadinessDto {
                        ready: false,
                        status: cadence_desktop_lib::commands::ProviderProfileReadinessStatusDto::Missing,
                        proof: None,
                        proof_updated_at: None,
                    },
                    migrated_from_legacy: false,
                    migrated_at: None,
                },
            ],
            migration: None,
        }),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            cadence_desktop_lib::commands::SET_ACTIVE_PROVIDER_PROFILE_COMMAND,
            json!({ "request": { "profileId": "zz-openai-alt" } }),
        ),
        Ok(cadence_desktop_lib::commands::ProviderProfilesDto {
            active_profile_id: "zz-openai-alt".into(),
            profiles: vec![
                cadence_desktop_lib::commands::ProviderProfileDto {
                    profile_id: "openai_codex-default".into(),
                    provider_id: "openai_codex".into(),
                    runtime_kind: "openai_codex".into(),
                    label: "OpenAI Codex".into(),
                    model_id: "openai_codex".into(),
                    preset_id: None,
                    base_url: None,
                    api_version: None,
            region: None,
            project_id: None,
                    active: false,
                    readiness: cadence_desktop_lib::commands::ProviderProfileReadinessDto {
                        ready: false,
                        status: cadence_desktop_lib::commands::ProviderProfileReadinessStatusDto::Missing,
                        proof: None,
                        proof_updated_at: None,
                    },
                    migrated_from_legacy: false,
                    migrated_at: None,
                },
                cadence_desktop_lib::commands::ProviderProfileDto {
                    profile_id: "zz-openai-alt".into(),
                    provider_id: "openai_codex".into(),
                    runtime_kind: "openai_codex".into(),
                    label: "OpenAI Alt".into(),
                    model_id: "openai_codex".into(),
                    preset_id: None,
                    base_url: None,
                    api_version: None,
            region: None,
            project_id: None,
                    active: true,
                    readiness: cadence_desktop_lib::commands::ProviderProfileReadinessDto {
                        ready: false,
                        status: cadence_desktop_lib::commands::ProviderProfileReadinessStatusDto::Missing,
                        proof: None,
                        proof_updated_at: None,
                    },
                    migrated_from_legacy: false,
                    migrated_at: None,
                },
            ],
            migration: None,
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
            anthropic_api_key_configured: false,
        }),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            START_OPENAI_CODEX_AUTH_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1",
                    "profileId": "openai_codex-default",
                    "originator": "tests"
                }
            }),
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
                    "profileId": "openai_codex-default",
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
            json!({ "request": { "projectId": "project-1", "agentSessionId": "agent-session-main" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            START_RUNTIME_RUN_COMMAND,
            json!({ "request": { "projectId": "project-1", "agentSessionId": "agent-session-main" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            CANCEL_AUTONOMOUS_RUN_COMMAND,
            json!({ "request": { "projectId": "project-1", "agentSessionId": "agent-session-main", "runId": "run-1" } }),
        ),
        Err(CommandError::project_not_found()),
    );

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            STOP_RUNTIME_RUN_COMMAND,
            json!({ "request": { "projectId": "project-1", "agentSessionId": "agent-session-main", "runId": "run-1" } }),
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
                    "agentSessionId": "agent-session-main",
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

pub(crate) fn config_and_capability_files_lock_the_packaged_vite_shell_and_auth_opener_permissions()
{
    let tauri_config: Value = serde_json::from_str(include_str!("../../tauri.conf.json"))
        .expect("valid tauri config json");
    let capability: Value = serde_json::from_str(include_str!("../../capabilities/default.json"))
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
        tauri_config["bundle"]["targets"],
        json!(["app"]),
        "tauri.conf bundle.targets drifted; debug release-gate builds must bundle only the macOS app artifact in deterministic local environments"
    );

    assert_eq!(
        capability["identifier"],
        json!("default"),
        "capabilities/default.json identifier drifted; bootstrap must lock to default"
    );
    assert_eq!(
        capability["webviews"],
        json!(["main"]),
        "capabilities/default.json webviews drifted; permission scope must stay bound to the main window"
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

const PLATFORM_MATRIX_RELEASE_GATE_COMMAND: &str = concat!(
    "cargo test --manifest-path client/src-tauri/Cargo.toml ",
    "--test runtime_session_bridge ",
    "--test autonomous_fixture_parity ",
    "--test runtime_event_stream ",
    "--test runtime_run_persistence ",
    "--test bootstrap_contracts && ",
    "pnpm --dir client test ",
    "src/features/cadence/use-cadence-desktop-state.runtime-run.test.tsx ",
    "src/features/cadence/live-views.test.tsx ",
    "components/cadence/agent-runtime.test.tsx && ",
    "cargo check --manifest-path client/src-tauri/Cargo.toml && ",
    "pnpm --dir client exec tauri build --debug"
);

pub(crate) fn platform_matrix_artifact_locks_cross_platform_verification_contract() {
    let matrix = include_str!("../platform-matrix.md");
    let expected_command_block = format!(
        "## Release-Gate Command (must match exactly on every target)\n\n```bash\n{}\n```",
        PLATFORM_MATRIX_RELEASE_GATE_COMMAND,
    );

    assert!(
        matrix.contains("milestone **M008** and slice **S06**"),
        "platform matrix artifact must lock the M008/S06 release gate instead of stale slice labels"
    );
    assert!(
        matrix.contains(&expected_command_block),
        "platform matrix artifact must lock the exact canonical release-gate command block"
    );
    assert_eq!(
        matrix.matches("```bash").count(),
        1,
        "platform matrix artifact must expose exactly one canonical release-gate command block"
    );
    assert!(
        matrix.contains("--test runtime_session_bridge"),
        "platform matrix artifact must keep runtime_session_bridge in the release-gate command"
    );
    assert!(
        matrix.contains("## macOS") && matrix.contains("## Linux") && matrix.contains("## Windows"),
        "platform matrix artifact must keep macOS, Linux, and Windows platform sections"
    );
    assert!(
        matrix.contains(
            "No platform-specific skips are allowed for this M008/S06 release-gate contract."
        ),
        "platform matrix artifact must explicitly forbid platform-specific skips"
    );
    assert!(
        !matrix.contains("S08")
            && !matrix.contains("autonomous_skill_runtime")
            && !matrix.contains("autonomous_imported_repo_bridge")
            && !matrix.contains("src/App.test.tsx"),
        "platform matrix artifact must reject stale release-gate labels and command fragments"
    );
}
