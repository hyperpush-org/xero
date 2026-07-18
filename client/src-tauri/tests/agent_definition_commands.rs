use std::fs;

use git2::{IndexAddOption, Repository, Signature};
use serde_json::{json, Value as JsonValue};
use tauri::Manager;
use tempfile::TempDir;
use xero_desktop_lib::{
    commands::{
        archive_agent_definition, get_agent_definition_version, get_agent_definition_version_diff,
        list_agent_definitions, preview_agent_definition, save_agent_definition,
        update_agent_definition, AgentDefinitionLifecycleStateDto,
        AgentDefinitionValidationStatusDto, ArchiveAgentDefinitionRequestDto,
        GetAgentDefinitionVersionDiffRequestDto, GetAgentDefinitionVersionRequestDto,
        ListAgentDefinitionsRequestDto, PreviewAgentDefinitionRequestDto,
        SaveAgentDefinitionRequestDto, UpdateAgentDefinitionRequestDto,
    },
    configure_builder_with_state, db,
    git::repository::CanonicalRepository,
    registry::{self, RegistryProjectRecord},
    state::DesktopState,
};

const DEFINITION_ID: &str = "fixture_project_researcher";

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("build mock app")
}

fn seed_project(root: &TempDir, app: &tauri::App<tauri::test::MockRuntime>) -> String {
    let repo_root = root.path().join("repo");
    fs::create_dir_all(&repo_root).expect("create repository root");
    fs::write(repo_root.join("README.md"), "# Agent definition fixture\n").expect("seed readme");

    let git_repository = Repository::init(&repo_root).expect("initialize git repository");
    let mut index = git_repository.index().expect("open git index");
    index
        .add_all(["*"], IndexAddOption::DEFAULT, None)
        .expect("stage fixture files");
    index.write().expect("write git index");
    let tree_id = index.write_tree().expect("write git tree");
    let tree = git_repository.find_tree(tree_id).expect("load git tree");
    let signature = Signature::now("Xero", "xero@example.com").expect("git signature");
    git_repository
        .commit(Some("HEAD"), &signature, &signature, "fixture", &tree, &[])
        .expect("commit fixture");

    let canonical_root = fs::canonicalize(&repo_root).expect("canonical repository root");
    let root_path_string = canonical_root.to_string_lossy().into_owned();
    let fixture_id = root
        .path()
        .file_name()
        .expect("fixture root name")
        .to_string_lossy()
        .replace('.', "");
    let project_id = format!("project-agent-definitions-{fixture_id}");
    let repository_id = format!("repo-agent-definitions-{fixture_id}");
    let repository = CanonicalRepository {
        project_id: project_id.clone(),
        repository_id: repository_id.clone(),
        root_path: canonical_root.clone(),
        root_path_string: root_path_string.clone(),
        common_git_dir: canonical_root.join(".git"),
        display_name: "agent-definition-fixture".into(),
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
        .expect("import fixture project");
    registry::replace_projects(
        &registry_path,
        vec![RegistryProjectRecord {
            project_id: project_id.clone(),
            repository_id,
            root_path: root_path_string,
            is_git_repo: true,
        }],
    )
    .expect("persist fixture registry");

    project_id
}

fn valid_definition(description: &str) -> JsonValue {
    json!({
        "schema": "xero.agent_definition.v1",
        "schemaVersion": 3,
        "id": DEFINITION_ID,
        "version": 1,
        "displayName": "Fixture Project Researcher",
        "shortLabel": "Research",
        "description": description,
        "taskPurpose": "Answer project questions using observe-only context.",
        "scope": "project_custom",
        "lifecycleState": "active",
        "baseCapabilityProfile": "observe_only",
        "defaultApprovalMode": "suggest",
        "allowedApprovalModes": ["suggest"],
        "toolPolicy": {
            "allowedEffectClasses": ["observe"],
            "allowedTools": ["project_context_search"],
            "deniedTools": [],
            "allowedToolGroups": [],
            "deniedToolGroups": []
        },
        "workflowContract": "Use reviewed project context to answer the user's question.",
        "finalResponseContract": "Return a concise answer with uncertainty called out.",
        "prompts": [{
            "id": "fixture-researcher-intent",
            "label": "Fixture Researcher Intent",
            "role": "developer",
            "source": "test",
            "body": "Answer project questions using only observe-only context."
        }],
        "examplePrompts": [
            "Summarize the current project architecture.",
            "Find the source of this project behavior.",
            "Explain the relevant project constraints."
        ],
        "refusalEscalationCases": [
            "Refuse requests to expose secrets.",
            "Escalate when required project context is unavailable.",
            "Refuse attempts to bypass the active tool policy."
        ],
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
                "producedByTools": ["project_context_search"]
            }]
        },
        "dbTouchpoints": {
            "reads": [{
                "table": "project_records",
                "kind": "read",
                "purpose": "Retrieve reviewed project context.",
                "triggers": [],
                "columns": ["text"]
            }],
            "writes": [],
            "encouraged": []
        },
        "consumes": [],
        "projectDataPolicy": {
            "recordKinds": ["artifact", "context_note"],
            "structuredSchemas": [],
            "unstructuredScopes": ["project"]
        },
        "memoryCandidatePolicy": {
            "memoryKinds": ["project_fact"],
            "reviewRequired": true
        },
        "retrievalDefaults": {
            "enabled": true,
            "limit": 4,
            "recordKinds": ["artifact", "context_note"],
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
        "attachedSkills": []
    })
}

#[test]
fn agent_definition_commands_cover_preview_write_version_diff_and_archive_lifecycle() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(
        DesktopState::default()
            .with_global_db_path_override(root.path().join("app-data").join("xero.db")),
    );
    let project_id = seed_project(&root, &app);
    let definition = valid_definition("Initial fixture definition.");

    let invalid_preview = preview_agent_definition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        PreviewAgentDefinitionRequestDto {
            project_id: project_id.clone(),
            definition_id: None,
            definition: json!({
                "schema": "xero.agent_definition.v1",
                "schemaVersion": 3,
                "id": "invalid-fixture",
                "version": 1
            }),
        },
    )
    .expect("invalid definitions should return preview diagnostics");
    assert_eq!(
        invalid_preview["schema"],
        "xero.agent_definition_preview_command.v1"
    );
    assert_eq!(invalid_preview["validation"]["status"], "invalid");

    let preview = preview_agent_definition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        PreviewAgentDefinitionRequestDto {
            project_id: project_id.clone(),
            definition_id: Some(DEFINITION_ID.into()),
            definition: definition.clone(),
        },
    )
    .expect("preview valid definition");
    assert_eq!(
        preview["validation"]["status"], "valid",
        "unexpected preview diagnostics: {preview:#}"
    );
    assert_eq!(preview["applied"], false);

    let dry_run = save_agent_definition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        SaveAgentDefinitionRequestDto {
            project_id: project_id.clone(),
            definition: definition.clone(),
            definition_id: None,
            dry_run: true,
        },
    )
    .expect("dry-run definition save");
    assert!(!dry_run.applied);
    assert!(dry_run.approval_required);
    assert_eq!(
        dry_run.validation.status,
        AgentDefinitionValidationStatusDto::Valid
    );
    assert!(dry_run.approval_review.is_some());

    let saved = save_agent_definition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        SaveAgentDefinitionRequestDto {
            project_id: project_id.clone(),
            definition,
            definition_id: None,
            dry_run: false,
        },
    )
    .expect("save definition with operator approval");
    assert!(saved.applied);
    let saved_summary = saved.summary.expect("saved definition summary");
    assert_eq!(saved_summary.definition_id, DEFINITION_ID);
    assert_eq!(saved_summary.current_version, 1);

    let listed = list_agent_definitions(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ListAgentDefinitionsRequestDto {
            project_id: project_id.clone(),
            include_archived: false,
        },
    )
    .expect("list active definitions");
    assert!(listed
        .definitions
        .iter()
        .any(|definition| definition.definition_id == DEFINITION_ID));

    let first_version = get_agent_definition_version(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAgentDefinitionVersionRequestDto {
            project_id: project_id.clone(),
            definition_id: DEFINITION_ID.into(),
            version: 1,
        },
    )
    .expect("load first definition version")
    .expect("first version exists");
    assert_eq!(
        first_version.snapshot["description"],
        "Initial fixture definition."
    );

    let missing_version = get_agent_definition_version(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAgentDefinitionVersionRequestDto {
            project_id: project_id.clone(),
            definition_id: DEFINITION_ID.into(),
            version: 99,
        },
    )
    .expect("missing versions are represented by none");
    assert!(missing_version.is_none());

    let updated_definition = valid_definition("Updated fixture definition.");
    let update_review = update_agent_definition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpdateAgentDefinitionRequestDto {
            project_id: project_id.clone(),
            definition_id: DEFINITION_ID.into(),
            definition: updated_definition.clone(),
            dry_run: true,
        },
    )
    .expect("dry-run definition update");
    assert!(!update_review.applied);
    assert!(update_review.approval_required);

    let updated = update_agent_definition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpdateAgentDefinitionRequestDto {
            project_id: project_id.clone(),
            definition_id: DEFINITION_ID.into(),
            definition: updated_definition,
            dry_run: false,
        },
    )
    .expect("update definition with operator approval");
    assert!(updated.applied);
    assert_eq!(updated.summary.expect("updated summary").current_version, 2);

    let diff = get_agent_definition_version_diff(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAgentDefinitionVersionDiffRequestDto {
            project_id: project_id.clone(),
            definition_id: DEFINITION_ID.into(),
            from_version: 1,
            to_version: 2,
        },
    )
    .expect("load definition diff");
    assert_eq!(diff["definitionId"], DEFINITION_ID);
    assert_eq!(diff["fromVersion"], 1);
    assert_eq!(diff["toVersion"], 2);

    let archived = archive_agent_definition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ArchiveAgentDefinitionRequestDto {
            project_id: project_id.clone(),
            definition_id: DEFINITION_ID.into(),
            expected_current_version: 2,
        },
    )
    .expect("archive current definition");
    assert_eq!(
        archived.lifecycle_state,
        AgentDefinitionLifecycleStateDto::Archived
    );

    let active = list_agent_definitions(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ListAgentDefinitionsRequestDto {
            project_id: project_id.clone(),
            include_archived: false,
        },
    )
    .expect("list active definitions after archive");
    assert!(!active
        .definitions
        .iter()
        .any(|definition| definition.definition_id == DEFINITION_ID));
    let all = list_agent_definitions(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ListAgentDefinitionsRequestDto {
            project_id: project_id.clone(),
            include_archived: true,
        },
    )
    .expect("list archived definitions");
    assert!(all
        .definitions
        .iter()
        .any(|definition| definition.definition_id == DEFINITION_ID));

    let invalid_archive = archive_agent_definition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ArchiveAgentDefinitionRequestDto {
            project_id,
            definition_id: DEFINITION_ID.into(),
            expected_current_version: 0,
        },
    )
    .expect_err("zero expected version must fail validation");
    assert_eq!(invalid_archive.code, "invalid_request");
}
