use std::fs;

use serde_json::json;
use tauri::Manager;
use tempfile::TempDir;
use xero_desktop_lib::{
    commands::{
        set_agent_default_model, AgentDefaultModelDto, AgentRefDto, ProviderModelThinkingEffortDto,
        RuntimeAgentIdDto, SetAgentDefaultModelRequestDto,
    },
    configure_builder_with_state,
    db::{self, project_store},
    git::repository::CanonicalRepository,
    registry::{self, RegistryProjectRecord},
    state::DesktopState,
};

const CUSTOM_DEFINITION_ID: &str = "default_model_researcher";

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("build mock app")
}

fn model(provider_id: &str, model_id: &str) -> AgentDefaultModelDto {
    AgentDefaultModelDto {
        provider_id: provider_id.into(),
        provider_profile_id: Some("profile-main".into()),
        model_id: model_id.into(),
        selection_key: Some(format!("{provider_id}:{model_id}")),
        thinking_effort: Some(ProviderModelThinkingEffortDto::High),
    }
}

fn built_in_request(default_model: Option<AgentDefaultModelDto>) -> SetAgentDefaultModelRequestDto {
    SetAgentDefaultModelRequestDto {
        project_id: "project-agent-default-model".into(),
        r#ref: AgentRefDto::BuiltIn {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            version: 1,
        },
        default_model,
    }
}

fn seed_custom_project(
    root: &TempDir,
    app: &tauri::App<tauri::test::MockRuntime>,
) -> (String, std::path::PathBuf) {
    let repo_root = root.path().join("repo");
    fs::create_dir_all(&repo_root).expect("create custom-model repository root");
    let canonical_root = fs::canonicalize(&repo_root).expect("canonical repository root");
    let suffix = root
        .path()
        .file_name()
        .expect("fixture suffix")
        .to_string_lossy()
        .replace('.', "");
    let project_id = format!("project-default-model-{suffix}");
    let repository_id = format!("repo-default-model-{suffix}");
    let root_path = canonical_root.to_string_lossy().into_owned();
    let repository = CanonicalRepository {
        project_id: project_id.clone(),
        repository_id: repository_id.clone(),
        root_path: canonical_root.clone(),
        root_path_string: root_path.clone(),
        common_git_dir: canonical_root.join(".git"),
        display_name: "default-model-fixture".into(),
        branch_name: None,
        head_sha: None,
        branch: None,
        last_commit: None,
        status_entries: Vec::new(),
        has_staged_changes: false,
        has_unstaged_changes: false,
        has_untracked_changes: false,
        additions: 0,
        deletions: 0,
    };
    let registry_path = app
        .state::<DesktopState>()
        .global_db_path(&app.handle().clone())
        .expect("registry path");
    db::configure_project_database_paths(&registry_path);
    db::import_project(&repository, app.state::<DesktopState>().import_failpoints())
        .expect("import custom-model fixture project");
    registry::replace_projects(
        &registry_path,
        vec![RegistryProjectRecord {
            project_id: project_id.clone(),
            repository_id,
            root_path,
            is_git_repo: false,
        }],
    )
    .expect("persist custom-model fixture registry");

    project_store::insert_agent_definition(
        &canonical_root,
        &project_store::NewAgentDefinitionRecord {
            definition_id: CUSTOM_DEFINITION_ID.into(),
            version: 1,
            display_name: "Default Model Researcher".into(),
            short_label: "Research".into(),
            description: "Answer project questions using observe-only context.".into(),
            scope: "project_custom".into(),
            lifecycle_state: "active".into(),
            base_capability_profile: "observe_only".into(),
            snapshot: json!({
                "schema": "xero.agent_definition.v1",
                "schemaVersion": 3,
                "id": CUSTOM_DEFINITION_ID,
                "version": 1,
                "displayName": "Default Model Researcher",
                "shortLabel": "Research",
                "description": "Answer project questions using observe-only context.",
                "taskPurpose": "Answer project questions using observe-only context.",
                "scope": "project_custom",
                "lifecycleState": "active",
                "baseCapabilityProfile": "observe_only",
                "defaultApprovalMode": "suggest",
                "allowedApprovalModes": ["suggest"],
                "toolPolicy": {
                    "allowedEffectClasses": ["observe"],
                    "allowedToolGroups": [],
                    "allowedToolPacks": [],
                    "allowedTools": ["read"],
                    "deniedTools": [],
                    "deniedToolPacks": [],
                    "externalServiceAllowed": false,
                    "browserControlAllowed": false,
                    "skillRuntimeAllowed": false,
                    "subagentAllowed": false,
                    "commandAllowed": false,
                    "destructiveWriteAllowed": false
                },
                "workflowContract": "Use reviewed project context to answer the user's question.",
                "finalResponseContract": "Return a concise answer with uncertainty called out.",
                "prompts": [{
                    "id": "default-model-researcher-intent",
                    "role": "developer",
                    "body": "Answer project questions using only observe-only context."
                }],
                "tools": [],
                "output": {
                    "contract": "answer",
                    "label": "Answer",
                    "description": "Answer the user's project question.",
                    "sections": [{
                        "id": "answer",
                        "label": "Answer",
                        "description": "Direct answer.",
                        "emphasis": "core",
                        "producedByTools": ["read"]
                    }]
                },
                "dbTouchpoints": {
                    "reads": [{
                        "table": "project_context_records",
                        "kind": "read",
                        "purpose": "Retrieve reviewed project context.",
                        "triggers": [],
                        "columns": ["summary"]
                    }],
                    "writes": [],
                    "encouraged": []
                },
                "consumes": [],
                "projectDataPolicy": {
                    "recordKinds": ["project_fact"],
                    "structuredSchemas": ["xero.project_record.v1"]
                },
                "memoryCandidatePolicy": {
                    "memoryKinds": ["project_fact"],
                    "reviewRequired": true
                },
                "retrievalDefaults": {
                    "enabled": true,
                    "limit": 6,
                    "recordKinds": ["project_fact"],
                    "memoryKinds": ["project_fact"]
                },
                "handoffPolicy": {
                    "enabled": true,
                    "routingMode": "same_agent",
                    "allowedTargets": [],
                    "preserveDefinitionVersion": true,
                    "carrySummary": true,
                    "includeDurableContext": true
                },
                "examplePrompts": [
                    "Draft release notes.",
                    "Summarize fixes.",
                    "List release risks."
                ],
                "refusalEscalationCases": [
                    "Refuse edits.",
                    "Escalate missing context.",
                    "Refuse invented claims."
                ],
                "attachedSkills": []
            }),
            validation_report: Some(json!({
                "status": "valid",
                "source": "custom_default_model_fixture"
            })),
            created_at: "2026-07-18T20:00:00Z".into(),
            updated_at: "2026-07-18T20:00:00Z".into(),
        },
    )
    .expect("insert custom-model fixture definition");
    (project_id, canonical_root)
}

#[test]
fn built_in_default_model_can_be_created_replaced_reset_and_validated() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(
        DesktopState::default()
            .with_global_db_path_override(root.path().join("app-data").join("xero.db")),
    );

    let first = model("provider-a", "model-a");
    let created = set_agent_default_model(
        app.handle().clone(),
        app.state::<DesktopState>(),
        built_in_request(Some(first.clone())),
    )
    .expect("create built-in default model");
    assert_eq!(created.default_model, Some(first));

    let replacement = model("provider-b", "model-b");
    let replaced = set_agent_default_model(
        app.handle().clone(),
        app.state::<DesktopState>(),
        built_in_request(Some(replacement.clone())),
    )
    .expect("replace built-in default model");
    assert_eq!(replaced.default_model, Some(replacement));

    let reset = set_agent_default_model(
        app.handle().clone(),
        app.state::<DesktopState>(),
        built_in_request(None),
    )
    .expect("reset built-in default model");
    assert_eq!(reset.default_model, None);

    let invalid_models = [
        AgentDefaultModelDto {
            provider_id: " ".into(),
            ..model("provider", "model")
        },
        AgentDefaultModelDto {
            model_id: "".into(),
            ..model("provider", "model")
        },
        AgentDefaultModelDto {
            provider_profile_id: Some("\n".into()),
            ..model("provider", "model")
        },
        AgentDefaultModelDto {
            selection_key: Some(" ".into()),
            ..model("provider", "model")
        },
    ];
    for invalid_model in invalid_models {
        let error = set_agent_default_model(
            app.handle().clone(),
            app.state::<DesktopState>(),
            built_in_request(Some(invalid_model)),
        )
        .expect_err("blank default-model identifiers must fail validation");
        assert_eq!(error.code, "invalid_request");
    }

    let invalid_project = set_agent_default_model(
        app.handle().clone(),
        app.state::<DesktopState>(),
        SetAgentDefaultModelRequestDto {
            project_id: " ".into(),
            ..built_in_request(Some(model("provider", "model")))
        },
    )
    .expect_err("blank project ids must fail validation");
    assert_eq!(invalid_project.code, "invalid_request");

    let missing_custom_project = set_agent_default_model(
        app.handle().clone(),
        app.state::<DesktopState>(),
        SetAgentDefaultModelRequestDto {
            project_id: "missing-project".into(),
            r#ref: AgentRefDto::Custom {
                definition_id: "missing-agent".into(),
                version: 1,
            },
            default_model: Some(model("provider", "model")),
        },
    )
    .expect_err("custom defaults require a registered project");
    assert_ne!(missing_custom_project.code, "invalid_request");
}

#[test]
fn custom_agent_default_model_updates_definition_versions_and_resets_cleanly() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(
        DesktopState::default()
            .with_global_db_path_override(root.path().join("app-data").join("xero.db")),
    );
    let (project_id, repo_root) = seed_custom_project(&root, &app);
    let custom_ref = AgentRefDto::Custom {
        definition_id: CUSTOM_DEFINITION_ID.into(),
        version: 1,
    };
    let expected = model("provider-custom", "model-custom");

    let saved = set_agent_default_model(
        app.handle().clone(),
        app.state::<DesktopState>(),
        SetAgentDefaultModelRequestDto {
            project_id: project_id.clone(),
            r#ref: custom_ref.clone(),
            default_model: Some(expected.clone()),
        },
    )
    .expect("save custom-agent default model");
    assert_eq!(saved.default_model, Some(expected.clone()));
    let current = project_store::load_agent_definition(&repo_root, CUSTOM_DEFINITION_ID)
        .expect("load custom definition")
        .expect("custom definition exists");
    assert_eq!(current.current_version, 2);
    let version_two = project_store::load_agent_definition_version(
        &repo_root,
        CUSTOM_DEFINITION_ID,
        current.current_version,
    )
    .expect("load custom definition version")
    .expect("custom version exists");
    assert_eq!(
        serde_json::from_value::<AgentDefaultModelDto>(
            version_two.snapshot["defaultModel"].clone()
        )
        .expect("decode persisted custom default model"),
        expected
    );

    let reset = set_agent_default_model(
        app.handle().clone(),
        app.state::<DesktopState>(),
        SetAgentDefaultModelRequestDto {
            project_id,
            r#ref: custom_ref,
            default_model: None,
        },
    )
    .expect("reset custom-agent default model");
    assert_eq!(reset.default_model, None);
    let reset_current = project_store::load_agent_definition(&repo_root, CUSTOM_DEFINITION_ID)
        .expect("load reset custom definition")
        .expect("reset custom definition exists");
    assert_eq!(reset_current.current_version, 3);
    let version_three = project_store::load_agent_definition_version(
        &repo_root,
        CUSTOM_DEFINITION_ID,
        reset_current.current_version,
    )
    .expect("load reset custom definition version")
    .expect("reset custom version exists");
    assert!(version_three.snapshot.get("defaultModel").is_none());
}
