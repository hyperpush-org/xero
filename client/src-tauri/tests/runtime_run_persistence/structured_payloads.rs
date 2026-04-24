use super::support::*;
pub(crate) fn autonomous_run_persistence_canonicalizes_structured_artifact_payloads_and_reloads_them(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Supervisor launched and connected to the project PTY.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run for structured artifact persistence");

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![sample_tool_result_artifact(project_id, run_id)];

    let persisted = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect("persist autonomous run with structured artifact");
    let artifact = &persisted.history[0].artifacts[0];
    let payload_hash = artifact
        .content_hash
        .as_ref()
        .expect("structured artifact should compute content hash")
        .clone();
    assert!(matches!(
        artifact.payload.as_ref(),
        Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(
            _
        ))
    ));

    let stored_payload_json: String = open_state_connection(&repo_root)
        .query_row(
            "SELECT payload_json FROM autonomous_unit_artifacts WHERE artifact_id = ?1",
            params![artifact.artifact_id.as_str()],
            |row| row.get(0),
        )
        .expect("read stored structured payload json");
    let expected_payload_json = concat!(
        "{",
        "\"actionId\":\"action-1\"",
        ",\"artifactId\":\"artifact-tool-result\"",
        ",\"attemptId\":\"run-1:unit:1:attempt:1\"",
        ",\"boundaryId\":\"boundary-1\"",
        ",\"commandResult\":{\"exitCode\":0,\"summary\":\"Command exited successfully after capturing structured evidence.\",\"timedOut\":false}",
        ",\"kind\":\"tool_result\"",
        ",\"projectId\":\"project-1\"",
        ",\"runId\":\"run-1\"",
        ",\"toolCallId\":\"tool-call-1\"",
        ",\"toolName\":\"shell.exec\"",
        ",\"toolState\":\"succeeded\"",
        ",\"toolSummary\":{\"exitCode\":0,\"kind\":\"command\",\"stderrRedacted\":false,\"stderrTruncated\":false,\"stdoutRedacted\":false,\"stdoutTruncated\":false,\"timedOut\":false}",
        ",\"unitId\":\"run-1:unit:1\"",
        "}"
    );
    assert_eq!(stored_payload_json, expected_payload_json);

    let mut hasher = Sha256::new();
    hasher.update(stored_payload_json.as_bytes());
    let expected_hash = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(payload_hash, expected_hash);

    let recovered = project_store::load_autonomous_run(&repo_root, project_id)
        .expect("reload autonomous run with structured artifact")
        .expect("structured autonomous run should exist");
    assert_eq!(
        recovered.history[0].artifacts[0].content_hash.as_deref(),
        Some(expected_hash.as_str())
    );
    assert!(matches!(
        recovered.history[0].artifacts[0].payload.as_ref(),
        Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(
            _
        ))
    ));
}

pub(crate) fn autonomous_run_persistence_canonicalizes_mcp_tool_result_payloads_and_reloads_them() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Supervisor launched and connected to the project PTY.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run for MCP structured artifact persistence");

    let mut artifact = sample_tool_result_artifact(project_id, run_id);
    if let Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(tool)) =
        artifact.payload.as_mut()
    {
        tool.tool_name = "mcp.invoke".into();
        tool.command_result = None;
        tool.tool_summary = Some(
            cadence_desktop_lib::runtime::protocol::ToolResultSummary::McpCapability(
                cadence_desktop_lib::runtime::protocol::McpCapabilityToolResultSummary {
                    server_id: "workspace-mcp".into(),
                    capability_kind:
                        cadence_desktop_lib::runtime::protocol::McpCapabilityKind::Prompt,
                    capability_id: "prompt://summarize".into(),
                    capability_name: Some("Summarize".into()),
                },
            ),
        );
    }

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![artifact];

    let persisted = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect("persist autonomous run with MCP structured artifact");
    let artifact = &persisted.history[0].artifacts[0];
    let payload_hash = artifact
        .content_hash
        .as_ref()
        .expect("MCP structured artifact should compute content hash")
        .clone();

    let stored_payload_json: String = open_state_connection(&repo_root)
        .query_row(
            "SELECT payload_json FROM autonomous_unit_artifacts WHERE artifact_id = ?1",
            params![artifact.artifact_id.as_str()],
            |row| row.get(0),
        )
        .expect("read stored MCP structured payload json");
    let expected_payload_json = concat!(
        "{",
        "\"actionId\":\"action-1\"",
        ",\"artifactId\":\"artifact-tool-result\"",
        ",\"attemptId\":\"run-1:unit:1:attempt:1\"",
        ",\"boundaryId\":\"boundary-1\"",
        ",\"commandResult\":null",
        ",\"kind\":\"tool_result\"",
        ",\"projectId\":\"project-1\"",
        ",\"runId\":\"run-1\"",
        ",\"toolCallId\":\"tool-call-1\"",
        ",\"toolName\":\"mcp.invoke\"",
        ",\"toolState\":\"succeeded\"",
        ",\"toolSummary\":{\"capabilityId\":\"prompt://summarize\",\"capabilityKind\":\"prompt\",\"capabilityName\":\"Summarize\",\"kind\":\"mcp_capability\",\"serverId\":\"workspace-mcp\"}",
        ",\"unitId\":\"run-1:unit:1\"",
        "}"
    );
    assert_eq!(stored_payload_json, expected_payload_json);

    let mut hasher = Sha256::new();
    hasher.update(stored_payload_json.as_bytes());
    let expected_hash = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(payload_hash, expected_hash);

    let recovered = project_store::load_autonomous_run(&repo_root, project_id)
        .expect("reload autonomous run with MCP structured artifact")
        .expect("MCP structured autonomous run should exist");
    let recovered_summary = recovered
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .find_map(|artifact| match artifact.payload.as_ref() {
            Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(payload)) => {
                payload.tool_summary.as_ref()
            }
            _ => None,
        })
        .expect("MCP tool summary should be present after reload");
    assert!(matches!(
        recovered_summary,
        cadence_desktop_lib::runtime::protocol::ToolResultSummary::McpCapability(summary)
            if summary.server_id == "workspace-mcp"
                && summary.capability_kind
                    == cadence_desktop_lib::runtime::protocol::McpCapabilityKind::Prompt
                && summary.capability_id == "prompt://summarize"
                && summary.capability_name.as_deref() == Some("Summarize")
    ));
}

pub(crate) fn autonomous_run_persistence_canonicalizes_browser_computer_use_tool_result_payloads_and_reloads_them(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Supervisor launched and connected to the project PTY.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run for browser/computer-use structured artifact persistence");

    let mut artifact = sample_tool_result_artifact(project_id, run_id);
    if let Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(tool)) =
        artifact.payload.as_mut()
    {
        tool.tool_name = "computer.drag".into();
        tool.tool_state = project_store::AutonomousToolCallStateRecord::Failed;
        tool.command_result = None;
        tool.tool_summary = Some(
            cadence_desktop_lib::runtime::protocol::ToolResultSummary::BrowserComputerUse(
                cadence_desktop_lib::runtime::protocol::BrowserComputerUseToolResultSummary {
                    surface:
                        cadence_desktop_lib::runtime::protocol::BrowserComputerUseSurface::ComputerUse,
                    action: "drag".into(),
                    status:
                        cadence_desktop_lib::runtime::protocol::BrowserComputerUseActionStatus::Blocked,
                    target: Some("Desktop icon".into()),
                    outcome: Some("Permission denied".into()),
                },
            ),
        );
    }

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![artifact];

    let persisted = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect("persist autonomous run with browser/computer-use structured artifact");
    let artifact = &persisted.history[0].artifacts[0];
    let payload_hash = artifact
        .content_hash
        .as_ref()
        .expect("browser/computer-use structured artifact should compute content hash")
        .clone();

    let stored_payload_json: String = open_state_connection(&repo_root)
        .query_row(
            "SELECT payload_json FROM autonomous_unit_artifacts WHERE artifact_id = ?1",
            params![artifact.artifact_id.as_str()],
            |row| row.get(0),
        )
        .expect("read stored browser/computer-use structured payload json");
    let expected_payload_json = concat!(
        "{",
        "\"actionId\":\"action-1\"",
        ",\"artifactId\":\"artifact-tool-result\"",
        ",\"attemptId\":\"run-1:unit:1:attempt:1\"",
        ",\"boundaryId\":\"boundary-1\"",
        ",\"commandResult\":null",
        ",\"kind\":\"tool_result\"",
        ",\"projectId\":\"project-1\"",
        ",\"runId\":\"run-1\"",
        ",\"toolCallId\":\"tool-call-1\"",
        ",\"toolName\":\"computer.drag\"",
        ",\"toolState\":\"failed\"",
        ",\"toolSummary\":{\"action\":\"drag\",\"kind\":\"browser_computer_use\",\"outcome\":\"Permission denied\",\"status\":\"blocked\",\"surface\":\"computer_use\",\"target\":\"Desktop icon\"}",
        ",\"unitId\":\"run-1:unit:1\"",
        "}"
    );
    assert_eq!(stored_payload_json, expected_payload_json);

    let mut hasher = Sha256::new();
    hasher.update(stored_payload_json.as_bytes());
    let expected_hash = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(payload_hash, expected_hash);

    let recovered = project_store::load_autonomous_run(&repo_root, project_id)
        .expect("reload autonomous run with browser/computer-use structured artifact")
        .expect("browser/computer-use structured autonomous run should exist");
    let recovered_summary = recovered
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .find_map(|artifact| match artifact.payload.as_ref() {
            Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(payload)) => {
                payload.tool_summary.as_ref()
            }
            _ => None,
        })
        .expect("browser/computer-use tool summary should be present after reload");
    assert!(matches!(
        recovered_summary,
        cadence_desktop_lib::runtime::protocol::ToolResultSummary::BrowserComputerUse(summary)
            if summary.surface
                == cadence_desktop_lib::runtime::protocol::BrowserComputerUseSurface::ComputerUse
                && summary.action == "drag"
                && summary.status
                    == cadence_desktop_lib::runtime::protocol::BrowserComputerUseActionStatus::Blocked
                && summary.target.as_deref() == Some("Desktop icon")
                && summary.outcome.as_deref() == Some("Permission denied")
    ));
}

pub(crate) fn autonomous_run_persistence_canonicalizes_verification_evidence_payloads_and_reloads_them(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Supervisor launched and connected to the project PTY.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run for verification evidence persistence");

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![sample_verification_evidence_artifact(project_id, run_id)];

    let persisted = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect("persist autonomous run with verification evidence artifact");
    let artifact = &persisted.history[0].artifacts[0];
    let payload_hash = artifact
        .content_hash
        .as_ref()
        .expect("verification evidence artifact should compute content hash")
        .clone();
    assert!(matches!(
        artifact.payload.as_ref(),
        Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(_))
    ));

    let stored_payload_json: String = open_state_connection(&repo_root)
        .query_row(
            "SELECT payload_json FROM autonomous_unit_artifacts WHERE artifact_id = ?1",
            params![artifact.artifact_id.as_str()],
            |row| row.get(0),
        )
        .expect("read stored verification evidence payload json");
    let expected_payload_json = concat!(
        "{",
        "\"actionId\":\"action-1\"",
        ",\"artifactId\":\"artifact-verification-evidence\"",
        ",\"attemptId\":\"run-1:unit:1:attempt:1\"",
        ",\"boundaryId\":\"boundary-1\"",
        ",\"commandResult\":null",
        ",\"evidenceKind\":\"terminal_input_required\"",
        ",\"kind\":\"verification_evidence\"",
        ",\"label\":\"Terminal input required\"",
        ",\"outcome\":\"blocked\"",
        ",\"projectId\":\"project-1\"",
        ",\"runId\":\"run-1\"",
        ",\"unitId\":\"run-1:unit:1\"",
        "}"
    );
    assert_eq!(stored_payload_json, expected_payload_json);

    let mut hasher = Sha256::new();
    hasher.update(stored_payload_json.as_bytes());
    let expected_hash = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(payload_hash, expected_hash);

    let recovered = project_store::load_autonomous_run(&repo_root, project_id)
        .expect("reload autonomous run with verification evidence artifact")
        .expect("verification evidence autonomous run should exist");
    assert_eq!(
        recovered.history[0].artifacts[0].content_hash.as_deref(),
        Some(expected_hash.as_str())
    );
    assert!(matches!(
        recovered.history[0].artifacts[0].payload.as_ref(),
        Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(_))
    ));
}

pub(crate) fn autonomous_run_persistence_canonicalizes_skill_lifecycle_payloads_and_reloads_them() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Supervisor launched and connected to the project PTY.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run for skill lifecycle persistence");

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![sample_skill_lifecycle_artifact(project_id, run_id)];

    let persisted = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect("persist autonomous run with skill lifecycle artifact");
    let artifact = &persisted.history[0].artifacts[0];
    let payload_hash = artifact
        .content_hash
        .as_ref()
        .expect("skill lifecycle artifact should compute content hash")
        .clone();
    assert!(matches!(
        artifact.payload.as_ref(),
        Some(project_store::AutonomousArtifactPayloadRecord::SkillLifecycle(_))
    ));

    let stored_payload_json: String = open_state_connection(&repo_root)
        .query_row(
            "SELECT payload_json FROM autonomous_unit_artifacts WHERE artifact_id = ?1",
            params![artifact.artifact_id.as_str()],
            |row| row.get(0),
        )
        .expect("read stored skill lifecycle payload json");
    let expected_payload_json = concat!(
        "{",
        "\"artifactId\":\"artifact-skill-lifecycle-discovery\"",
        ",\"attemptId\":\"run-1:unit:1:attempt:1\"",
        ",\"cache\":{\"key\":\"find-skills-576b45048241\"}",
        ",\"kind\":\"skill_lifecycle\"",
        ",\"projectId\":\"project-1\"",
        ",\"result\":\"succeeded\"",
        ",\"runId\":\"run-1\"",
        ",\"skillId\":\"find-skills\"",
        ",\"source\":{\"path\":\"skills/find-skills\",\"reference\":\"main\",\"repo\":\"vercel-labs/skills\",\"treeHash\":\"0123456789abcdef0123456789abcdef01234567\"}",
        ",\"stage\":\"discovery\"",
        ",\"unitId\":\"run-1:unit:1\"",
        "}"
    );
    assert_eq!(stored_payload_json, expected_payload_json);

    let mut hasher = Sha256::new();
    hasher.update(stored_payload_json.as_bytes());
    let expected_hash = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(payload_hash, expected_hash);

    let recovered = project_store::load_autonomous_run(&repo_root, project_id)
        .expect("reload autonomous run with skill lifecycle artifact")
        .expect("skill lifecycle autonomous run should exist");
    assert_eq!(
        recovered.history[0].artifacts[0].content_hash.as_deref(),
        Some(expected_hash.as_str())
    );
    assert!(matches!(
        recovered.history[0].artifacts[0].payload.as_ref(),
        Some(project_store::AutonomousArtifactPayloadRecord::SkillLifecycle(_))
    ));
}

pub(crate) fn autonomous_run_persistence_rejects_verification_evidence_action_boundary_mismatch() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Bootstrap checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run for verification evidence linkage mismatch");

    let mut artifact = sample_verification_evidence_artifact(project_id, run_id);
    if let Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(evidence)) =
        artifact.payload.as_mut()
    {
        evidence.boundary_id = None;
    }

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![artifact];

    let error = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect_err("verification evidence action/boundary mismatch should be rejected");
    assert_eq!(error.code, "autonomous_run_request_invalid");
}

pub(crate) fn autonomous_run_persistence_rejects_structured_artifact_payload_linkage_mismatch() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Bootstrap checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run for linkage mismatch");

    let mut artifact = sample_tool_result_artifact(project_id, run_id);
    if let Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(tool)) =
        artifact.payload.as_mut()
    {
        tool.attempt_id = "run-1:unit:1:attempt:99".into();
    }

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![artifact];

    let error = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect_err("payload linkage mismatch should be rejected");
    assert_eq!(error.code, "autonomous_run_request_invalid");
}

pub(crate) fn autonomous_run_persistence_rejects_mcp_tool_summary_with_command_result() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Bootstrap checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run for MCP command-result mismatch");

    let mut artifact = sample_tool_result_artifact(project_id, run_id);
    if let Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(tool)) =
        artifact.payload.as_mut()
    {
        tool.tool_name = "mcp.invoke".into();
        tool.tool_summary = Some(
            cadence_desktop_lib::runtime::protocol::ToolResultSummary::McpCapability(
                cadence_desktop_lib::runtime::protocol::McpCapabilityToolResultSummary {
                    server_id: "workspace-mcp".into(),
                    capability_kind:
                        cadence_desktop_lib::runtime::protocol::McpCapabilityKind::Tool,
                    capability_id: "workspace/list".into(),
                    capability_name: Some("list".into()),
                },
            ),
        );
    }

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![artifact];

    let error = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect_err("MCP tool summary with command_result should be rejected");
    assert_eq!(error.code, "autonomous_run_request_invalid");
}

pub(crate) fn autonomous_run_persistence_rejects_browser_computer_use_tool_summary_with_command_result(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Bootstrap checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run for browser/computer-use command-result mismatch");

    let mut artifact = sample_tool_result_artifact(project_id, run_id);
    if let Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(tool)) =
        artifact.payload.as_mut()
    {
        tool.tool_name = "browser.click".into();
        tool.tool_summary = Some(
            cadence_desktop_lib::runtime::protocol::ToolResultSummary::BrowserComputerUse(
                cadence_desktop_lib::runtime::protocol::BrowserComputerUseToolResultSummary {
                    surface:
                        cadence_desktop_lib::runtime::protocol::BrowserComputerUseSurface::Browser,
                    action: "click".into(),
                    status:
                        cadence_desktop_lib::runtime::protocol::BrowserComputerUseActionStatus::Running,
                    target: Some("button#primary".into()),
                    outcome: None,
                },
            ),
        );
    }

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![artifact];

    let error = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect_err("browser/computer-use tool summary with command_result should be rejected");
    assert_eq!(error.code, "autonomous_run_request_invalid");
}

pub(crate) fn autonomous_run_persistence_rejects_browser_computer_use_status_tool_state_mismatch() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Bootstrap checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run for browser/computer-use status mismatch");

    let mut artifact = sample_tool_result_artifact(project_id, run_id);
    if let Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(tool)) =
        artifact.payload.as_mut()
    {
        tool.tool_name = "browser.click".into();
        tool.tool_state = project_store::AutonomousToolCallStateRecord::Failed;
        tool.command_result = None;
        tool.tool_summary = Some(
            cadence_desktop_lib::runtime::protocol::ToolResultSummary::BrowserComputerUse(
                cadence_desktop_lib::runtime::protocol::BrowserComputerUseToolResultSummary {
                    surface:
                        cadence_desktop_lib::runtime::protocol::BrowserComputerUseSurface::Browser,
                    action: "click".into(),
                    status:
                        cadence_desktop_lib::runtime::protocol::BrowserComputerUseActionStatus::Succeeded,
                    target: Some("button#primary".into()),
                    outcome: Some("Clicked".into()),
                },
            ),
        );
    }

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![artifact];

    let error = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect_err("browser/computer-use status/tool_state mismatch should be rejected");
    assert_eq!(error.code, "autonomous_run_request_invalid");
}

pub(crate) fn autonomous_run_persistence_rejects_oversized_browser_computer_use_summary_fields() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Bootstrap checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run for oversized browser/computer-use summary rejection");

    let mut artifact = sample_tool_result_artifact(project_id, run_id);
    if let Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(tool)) =
        artifact.payload.as_mut()
    {
        tool.tool_name = "browser.click".into();
        tool.tool_state = project_store::AutonomousToolCallStateRecord::Running;
        tool.command_result = None;
        tool.tool_summary = Some(
            cadence_desktop_lib::runtime::protocol::ToolResultSummary::BrowserComputerUse(
                cadence_desktop_lib::runtime::protocol::BrowserComputerUseToolResultSummary {
                    surface:
                        cadence_desktop_lib::runtime::protocol::BrowserComputerUseSurface::Browser,
                    action: "x".repeat(513),
                    status:
                        cadence_desktop_lib::runtime::protocol::BrowserComputerUseActionStatus::Running,
                    target: Some("button#primary".into()),
                    outcome: None,
                },
            ),
        );
    }

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![artifact];

    let error = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect_err("oversized browser/computer-use summary text should be rejected");
    assert_eq!(error.code, "autonomous_run_request_invalid");
}

pub(crate) fn autonomous_run_decode_fails_closed_when_mcp_capability_kind_is_tampered() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Bootstrap checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run before MCP payload tampering");

    let mut artifact = sample_tool_result_artifact(project_id, run_id);
    if let Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(tool)) =
        artifact.payload.as_mut()
    {
        tool.tool_name = "mcp.invoke".into();
        tool.command_result = None;
        tool.tool_summary = Some(
            cadence_desktop_lib::runtime::protocol::ToolResultSummary::McpCapability(
                cadence_desktop_lib::runtime::protocol::McpCapabilityToolResultSummary {
                    server_id: "workspace-mcp".into(),
                    capability_kind:
                        cadence_desktop_lib::runtime::protocol::McpCapabilityKind::Prompt,
                    capability_id: "prompt://summarize".into(),
                    capability_name: Some("Summarize".into()),
                },
            ),
        );
    }

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![artifact];
    project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect("persist MCP structured artifact before tampering");

    open_state_connection(&repo_root)
        .execute(
            "UPDATE autonomous_unit_artifacts SET payload_json = ?1 WHERE artifact_id = ?2",
            params![
                concat!(
                    "{",
                    "\"kind\":\"tool_result\"",
                    ",\"projectId\":\"project-1\"",
                    ",\"runId\":\"run-1\"",
                    ",\"unitId\":\"run-1:unit:1\"",
                    ",\"attemptId\":\"run-1:unit:1:attempt:1\"",
                    ",\"artifactId\":\"artifact-tool-result\"",
                    ",\"toolCallId\":\"tool-call-1\"",
                    ",\"toolName\":\"mcp.invoke\"",
                    ",\"toolState\":\"succeeded\"",
                    ",\"commandResult\":null",
                    ",\"toolSummary\":{\"kind\":\"mcp_capability\",\"serverId\":\"workspace-mcp\",\"capabilityKind\":\"workflow\",\"capabilityId\":\"prompt://summarize\",\"capabilityName\":\"Summarize\"}",
                    ",\"actionId\":\"action-1\"",
                    ",\"boundaryId\":\"boundary-1\"",
                    "}"
                ),
                "artifact-tool-result"
            ],
        )
        .expect("tamper MCP tool summary capability kind");

    let error = project_store::load_autonomous_run(&repo_root, project_id)
        .expect_err("tampered MCP capability kind should fail closed");
    assert_eq!(error.code, "runtime_run_decode_failed");
}

pub(crate) fn autonomous_run_decode_fails_closed_when_browser_computer_use_summary_is_tampered() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Bootstrap checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run before browser/computer-use payload tampering");

    let mut artifact = sample_tool_result_artifact(project_id, run_id);
    if let Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(tool)) =
        artifact.payload.as_mut()
    {
        tool.tool_name = "browser.click".into();
        tool.tool_state = project_store::AutonomousToolCallStateRecord::Running;
        tool.command_result = None;
        tool.tool_summary = Some(
            cadence_desktop_lib::runtime::protocol::ToolResultSummary::BrowserComputerUse(
                cadence_desktop_lib::runtime::protocol::BrowserComputerUseToolResultSummary {
                    surface:
                        cadence_desktop_lib::runtime::protocol::BrowserComputerUseSurface::Browser,
                    action: "click".into(),
                    status:
                        cadence_desktop_lib::runtime::protocol::BrowserComputerUseActionStatus::Running,
                    target: Some("button#primary".into()),
                    outcome: None,
                },
            ),
        );
    }

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![artifact];
    project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect("persist browser/computer-use structured artifact before tampering");

    open_state_connection(&repo_root)
        .execute(
            "UPDATE autonomous_unit_artifacts SET payload_json = ?1 WHERE artifact_id = ?2",
            params![
                concat!(
                    "{",
                    "\"kind\":\"tool_result\"",
                    ",\"projectId\":\"project-1\"",
                    ",\"runId\":\"run-1\"",
                    ",\"unitId\":\"run-1:unit:1\"",
                    ",\"attemptId\":\"run-1:unit:1:attempt:1\"",
                    ",\"artifactId\":\"artifact-tool-result\"",
                    ",\"toolCallId\":\"tool-call-1\"",
                    ",\"toolName\":\"browser.click\"",
                    ",\"toolState\":\"running\"",
                    ",\"commandResult\":null",
                    ",\"toolSummary\":{\"kind\":\"browser_computer_use\",\"surface\":\"browser\",\"action\":\"click\",\"status\":\"queued\",\"target\":\"button#primary\",\"outcome\":null}",
                    ",\"actionId\":\"action-1\"",
                    ",\"boundaryId\":\"boundary-1\"",
                    "}"
                ),
                "artifact-tool-result"
            ],
        )
        .expect("tamper browser/computer-use tool summary status");

    let error = project_store::load_autonomous_run(&repo_root, project_id)
        .expect_err("tampered browser/computer-use summary should fail closed");
    assert_eq!(error.code, "runtime_run_decode_failed");
}

pub(crate) fn autonomous_run_persistence_rejects_secret_bearing_structured_payload_content() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Bootstrap checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run for secret-bearing payload rejection");

    let mut artifact = sample_tool_result_artifact(project_id, run_id);
    if let Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(tool)) =
        artifact.payload.as_mut()
    {
        if let Some(command_result) = tool.command_result.as_mut() {
            command_result.summary = "Authorization: Bearer sk-secret-token".into();
        }
    }

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![artifact];

    let error = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect_err("secret-bearing payload should be rejected");
    assert_eq!(error.code, "autonomous_run_request_invalid");
}

pub(crate) fn autonomous_run_persistence_rejects_skill_lifecycle_payloads_without_tree_hash() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Bootstrap checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run for missing tree hash rejection");

    let mut artifact = sample_skill_lifecycle_artifact(project_id, run_id);
    if let Some(project_store::AutonomousArtifactPayloadRecord::SkillLifecycle(skill)) =
        artifact.payload.as_mut()
    {
        skill.source.tree_hash.clear();
    }

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![artifact];

    let error = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect_err("missing skill tree hash should be rejected");
    assert_eq!(error.code, "autonomous_run_request_invalid");
}

pub(crate) fn autonomous_run_persistence_rejects_skill_lifecycle_kind_mismatch() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Bootstrap checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run for kind mismatch rejection");

    let mut artifact = sample_skill_lifecycle_artifact(project_id, run_id);
    artifact.artifact_kind = "tool_result".into();

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![artifact];

    let error = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect_err("mismatched artifact kind should be rejected");
    assert_eq!(error.code, "autonomous_run_request_invalid");
}

pub(crate) fn autonomous_run_persistence_rejects_successful_skill_lifecycle_payloads_with_diagnostics(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Bootstrap checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run for invalid success/diagnostic rejection");

    let mut artifact = sample_skill_lifecycle_artifact(project_id, run_id);
    if let Some(project_store::AutonomousArtifactPayloadRecord::SkillLifecycle(skill)) =
        artifact.payload.as_mut()
    {
        skill.diagnostic = Some(project_store::AutonomousSkillLifecycleDiagnosticRecord {
            code: "autonomous_skill_source_timeout".into(),
            message: "Cadence timed out while contacting the autonomous skill source.".into(),
            retryable: true,
        });
    }

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![artifact];

    let error = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect_err("successful skill lifecycle payload with diagnostics should be rejected");
    assert_eq!(error.code, "autonomous_run_request_invalid");
}

pub(crate) fn autonomous_run_persistence_rejects_policy_denied_artifacts_without_stable_code() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";
    let unit_id = format!("{run_id}:unit:1");
    let attempt_id = format!("{run_id}:unit:1:attempt:1");

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Bootstrap checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run for policy denial rejection");

    let artifact = project_store::AutonomousUnitArtifactRecord {
        project_id: project_id.into(),
        run_id: run_id.into(),
        unit_id: unit_id.clone(),
        attempt_id: attempt_id.clone(),
        artifact_id: "artifact-policy-denied".into(),
        artifact_kind: "policy_denied".into(),
        status: project_store::AutonomousUnitArtifactStatus::Rejected,
        summary: "Policy denied shell write access for the executor attempt.".into(),
        content_hash: None,
        payload: Some(
            project_store::AutonomousArtifactPayloadRecord::PolicyDenied(
                project_store::AutonomousPolicyDeniedPayloadRecord {
                    project_id: project_id.into(),
                    run_id: run_id.into(),
                    unit_id,
                    attempt_id,
                    artifact_id: "artifact-policy-denied".into(),
                    diagnostic_code: "   ".into(),
                    message: "Policy denied write access to the repository worktree.".into(),
                    tool_name: Some("shell.exec".into()),
                    action_id: Some("action-1".into()),
                    boundary_id: Some("boundary-1".into()),
                },
            ),
        ),
        created_at: "2099-04-15T19:00:20Z".into(),
        updated_at: "2099-04-15T19:00:20Z".into(),
    };

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![artifact];

    let error = project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect_err("policy_denied artifact without diagnostic code should be rejected");
    assert_eq!(error.code, "policy_denied");
}

pub(crate) fn autonomous_run_decode_fails_closed_when_structured_payload_json_is_tampered() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Bootstrap checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run before payload tampering");

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![sample_tool_result_artifact(project_id, run_id)];
    project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect("persist structured artifact before tampering");

    open_state_connection(&repo_root)
        .execute(
            "UPDATE autonomous_unit_artifacts SET payload_json = ?1 WHERE artifact_id = ?2",
            params![
                "{\"kind\":\"tool_result\",\"toolCallId\":",
                "artifact-tool-result"
            ],
        )
        .expect("tamper structured payload json");

    let error = project_store::load_autonomous_run(&repo_root, project_id)
        .expect_err("malformed payload json should fail closed");
    assert_eq!(error.code, "runtime_run_decode_failed");
}

pub(crate) fn autonomous_run_decode_fails_closed_when_skill_lifecycle_payload_stage_is_tampered() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Bootstrap checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run before skill lifecycle tampering");

    let mut payload = sample_autonomous_run(project_id, run_id);
    payload.artifacts = vec![sample_skill_lifecycle_artifact(project_id, run_id)];
    project_store::upsert_autonomous_run(&repo_root, &payload)
        .expect("persist skill lifecycle artifact before tampering");

    open_state_connection(&repo_root)
        .execute(
            "UPDATE autonomous_unit_artifacts SET payload_json = ?1 WHERE artifact_id = ?2",
            params![
                concat!(
                    "{",
                    "\"kind\":\"skill_lifecycle\"",
                    ",\"projectId\":\"project-1\"",
                    ",\"runId\":\"run-1\"",
                    ",\"unitId\":\"run-1:unit:1\"",
                    ",\"attemptId\":\"run-1:unit:1:attempt:1\"",
                    ",\"artifactId\":\"artifact-skill-lifecycle-discovery\"",
                    ",\"stage\":\"discover\"",
                    ",\"result\":\"succeeded\"",
                    ",\"skillId\":\"find-skills\"",
                    ",\"source\":{\"repo\":\"vercel-labs/skills\",\"path\":\"skills/find-skills\",\"reference\":\"main\",\"treeHash\":\"0123456789abcdef0123456789abcdef01234567\"}",
                    ",\"cache\":{\"key\":\"find-skills-576b45048241\"}",
                    "}"
                ),
                "artifact-skill-lifecycle-discovery"
            ],
        )
        .expect("tamper skill lifecycle payload stage");

    let error = project_store::load_autonomous_run(&repo_root, project_id)
        .expect_err("tampered skill lifecycle payload should fail closed");
    assert_eq!(error.code, "runtime_run_decode_failed");
}

pub(crate) fn autonomous_skill_lifecycle_persistence_is_replay_safe_across_stage_upserts() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let run_id = "run-1";

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: sample_run(project_id, run_id),
            checkpoint: Some(sample_checkpoint(
                project_id,
                run_id,
                1,
                project_store::RuntimeRunCheckpointKind::Bootstrap,
                "Bootstrap checkpoint.",
                "2099-04-15T19:00:20Z",
            )),
            control_state: Some(sample_control_state("2099-04-15T19:00:00Z")),
        },
    )
    .expect("persist runtime run for replay-safe skill lifecycle persistence");

    let discovery =
        AutonomousSkillLifecycleEvent::discovered("find-skills", sample_skill_source_metadata());
    persist_skill_lifecycle_event(&repo_root, project_id, &discovery)
        .expect("persist discovery skill lifecycle event")
        .expect("skill lifecycle snapshot should exist");

    let after_discovery = project_store::load_autonomous_run(&repo_root, project_id)
        .expect("load autonomous run after discovery")
        .expect("autonomous run should exist after discovery");
    let discovery_artifact = after_discovery
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .find(|artifact| artifact.artifact_kind == "skill_lifecycle")
        .expect("discovery artifact should exist")
        .clone();

    persist_skill_lifecycle_event(&repo_root, project_id, &discovery)
        .expect("persist repeated discovery skill lifecycle event")
        .expect("skill lifecycle snapshot should still exist");

    let install = AutonomousSkillLifecycleEvent {
        stage: project_store::AutonomousSkillLifecycleStageRecord::Install,
        result: project_store::AutonomousSkillLifecycleResultRecord::Succeeded,
        skill_id: "find-skills".into(),
        source: sample_skill_source_metadata(),
        cache_key: "find-skills-576b45048241".into(),
        cache_status: Some(AutonomousSkillCacheStatus::Hit),
        diagnostic: None,
    };
    persist_skill_lifecycle_event(&repo_root, project_id, &install)
        .expect("persist install skill lifecycle event")
        .expect("skill lifecycle snapshot should still exist");

    let recovered = project_store::load_autonomous_run(&repo_root, project_id)
        .expect("reload autonomous run after repeated stage writes")
        .expect("autonomous run should exist after repeated stage writes");
    let skill_artifacts = recovered
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .filter(|artifact| artifact.artifact_kind == "skill_lifecycle")
        .collect::<Vec<_>>();

    assert_eq!(
        skill_artifacts.len(),
        2,
        "expected one discovery row and one install row"
    );
    let repeated_discovery = skill_artifacts
        .iter()
        .find(|artifact| artifact.artifact_id == discovery_artifact.artifact_id)
        .expect("repeated discovery artifact should still exist");
    assert_eq!(repeated_discovery.created_at, discovery_artifact.created_at);
    assert!(skill_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::SkillLifecycle(payload))
                if payload.stage == project_store::AutonomousSkillLifecycleStageRecord::Install
                    && payload.result
                        == project_store::AutonomousSkillLifecycleResultRecord::Succeeded
        )
    }));
}
