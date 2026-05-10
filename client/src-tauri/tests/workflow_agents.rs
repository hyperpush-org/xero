use std::{
    fs,
    path::{Path, PathBuf},
};

use git2::{IndexAddOption, Repository, Signature};
use serde_json::json;
use tauri::Manager;
use tempfile::TempDir;
use xero_desktop_lib::{
    commands::{
        get_agent_authoring_catalog, get_workflow_agent_detail, list_workflow_agents,
        AgentDbTouchpointKindDto, AgentDefinitionScopeDto, AgentOutputSectionEmphasisDto,
        AgentRefDto, AgentToolEffectClassDto, AgentTriggerRefDto,
        GetAgentAuthoringCatalogRequestDto, GetWorkflowAgentDetailRequestDto,
        ListWorkflowAgentsRequestDto, RuntimeAgentIdDto, RuntimeAgentOutputContractDto,
    },
    configure_builder_with_state,
    db::{self, project_store},
    git::repository::CanonicalRepository,
    registry::{self, RegistryProjectRecord},
    runtime::{
        XeroSkillSourceLocator, XeroSkillSourceRecord, XeroSkillSourceScope, XeroSkillSourceState,
        XeroSkillTrustState,
    },
    state::DesktopState,
};

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn create_state(root: &TempDir) -> DesktopState {
    DesktopState::default()
        .with_global_db_path_override(root.path().join("app-data").join("xero.db"))
}

fn seed_project(root: &TempDir, app: &tauri::App<tauri::test::MockRuntime>) -> (String, PathBuf) {
    let repo_root = root.path().join("repo");
    fs::create_dir_all(repo_root.join("src")).expect("create repo src");
    fs::write(repo_root.join("src").join("tracked.txt"), "alpha\n").expect("seed tracked file");

    let git_repository = Repository::init(&repo_root).expect("init git repo");
    commit_all(&git_repository, "initial commit");

    let canonical_root = fs::canonicalize(&repo_root).expect("canonical repo root");
    let root_path_string = canonical_root.to_string_lossy().into_owned();
    let repository = CanonicalRepository {
        project_id: "project-workflow-agents".into(),
        repository_id: "repo-workflow-agents".into(),
        root_path: canonical_root.clone(),
        root_path_string: root_path_string.clone(),
        common_git_dir: canonical_root.join(".git"),
        display_name: "repo".into(),
        branch_name: current_branch_name(&canonical_root),
        head_sha: current_head_sha(&canonical_root),
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
        .expect("import project into app-data db");

    registry::replace_projects(
        &registry_path,
        vec![RegistryProjectRecord {
            project_id: repository.project_id.clone(),
            repository_id: repository.repository_id.clone(),
            root_path: root_path_string,
        }],
    )
    .expect("persist registry entry");

    (repository.project_id, canonical_root)
}

fn commit_all(repository: &Repository, message: &str) {
    let mut index = repository.index().expect("repo index");
    index
        .add_all(["*"], IndexAddOption::DEFAULT, None)
        .expect("stage files");
    index.write().expect("write index");

    let tree_id = index.write_tree().expect("write tree");
    let tree = repository.find_tree(tree_id).expect("find tree");
    let signature = Signature::now("Xero", "xero@example.com").expect("signature");

    repository
        .commit(Some("HEAD"), &signature, &signature, message, &tree, &[])
        .expect("commit");
}

fn current_branch_name(repo_root: &Path) -> Option<String> {
    Repository::open(repo_root).ok().and_then(|repository| {
        repository
            .head()
            .ok()
            .and_then(|head| head.shorthand().map(ToOwned::to_owned))
    })
}

fn current_head_sha(repo_root: &Path) -> Option<String> {
    Repository::open(repo_root).ok().and_then(|repository| {
        repository
            .head()
            .ok()
            .and_then(|head| head.target().map(|oid| oid.to_string()))
    })
}

fn seed_custom_definition(repo_root: &Path) -> String {
    let definition_id = "custom_security_reviewer".to_string();
    project_store::insert_agent_definition(
        repo_root,
        &project_store::NewAgentDefinitionRecord {
            definition_id: definition_id.clone(),
            version: 1,
            display_name: "Security Reviewer".into(),
            short_label: "SecRev".into(),
            description: "Reviews diffs for threat-model coverage.".into(),
            scope: "project_custom".into(),
            lifecycle_state: "active".into(),
            base_capability_profile: "engineering".into(),
            snapshot: json!({
                "schema": "xero.agent_definition.v1",
                "schemaVersion": 2,
                "id": definition_id,
                "version": 1,
                "displayName": "Security Reviewer",
                "shortLabel": "SecRev",
                "description": "Reviews diffs for threat-model coverage.",
                "taskPurpose": "Audit changes against the threat model and propose mitigations.",
                "scope": "project_custom",
                "lifecycleState": "active",
                "baseCapabilityProfile": "engineering",
                "defaultApprovalMode": "suggest",
                "allowedApprovalModes": ["suggest", "auto_edit"],
                "toolPolicy": {
                    "allowedEffectClasses": ["observe", "runtime_state", "write", "command"],
                    "allowedToolGroups": ["core", "mutation", "command_readonly"],
                    "allowedTools": [],
                    "deniedTools": [],
                    "externalServiceAllowed": false,
                    "browserControlAllowed": false,
                    "skillRuntimeAllowed": false,
                    "subagentAllowed": false,
                    "commandAllowed": true,
                    "destructiveWriteAllowed": false
                },
                "promptFragments": [
                    {
                        "id": "security.coverage",
                        "title": "Coverage policy",
                        "body": "Always cite the threat model section the diff intersects."
                    }
                ],
                "workflowContract": "Read diff, map to threats, propose mitigations.",
                "finalResponseContract": "Return a numbered list of mitigations.",
                "examplePrompts": [
                    "Review this diff for security risks.",
                    "Map the recent changes to the threat model.",
                    "Summarize mitigations for this patch."
                ],
                "refusalEscalationCases": [
                    "Refuse to expose secrets.",
                    "Escalate missing threat-model context.",
                    "Refuse to bypass approval policy."
                ],
                "attachedSkills": [],
                "output": {
                    "contract": "engineering_summary",
                    "label": "Mitigations",
                    "description": "Threat-model mitigations.",
                    "sections": [
                        {
                            "id": "mitigations",
                            "label": "Mitigations",
                            "description": "Numbered mitigation list.",
                            "emphasis": "core",
                            "producedByTools": []
                        }
                    ]
                },
                "dbTouchpoints": {
                    "reads": [],
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
                    "preserveDefinitionVersion": true
                }
            }),
            validation_report: Some(json!({ "status": "valid" })),
            created_at: "2026-05-07T10:00:00Z".into(),
            updated_at: "2026-05-07T10:00:00Z".into(),
        },
    )
    .expect("insert custom definition");
    definition_id
}

fn seed_installed_skill(repo_root: &Path, source_state: XeroSkillSourceState) -> String {
    seed_installed_skill_with_id(repo_root, "rust-best-practices", "1.0.0", source_state)
}

fn seed_installed_skill_with_id(
    repo_root: &Path,
    skill_id: &str,
    version: &str,
    source_state: XeroSkillSourceState,
) -> String {
    let source = XeroSkillSourceRecord::new(
        XeroSkillSourceScope::global(),
        XeroSkillSourceLocator::Bundled {
            bundle_id: "xero".into(),
            skill_id: skill_id.into(),
            version: version.into(),
        },
        source_state,
        XeroSkillTrustState::Trusted,
    )
    .expect("skill source");
    let source_id = source.source_id.clone();
    project_store::upsert_installed_skill(
        repo_root,
        project_store::InstalledSkillRecord {
            source,
            skill_id: skill_id.into(),
            name: format!("{skill_id} skill"),
            description: format!("Guide for {skill_id}."),
            user_invocable: Some(true),
            cache_key: None,
            local_location: Some(format!("/tmp/xero-{skill_id}")),
            version_hash: Some(format!("version-hash-{skill_id}")),
            installed_at: "2026-05-07T10:00:00Z".into(),
            updated_at: "2026-05-07T10:00:00Z".into(),
            last_used_at: None,
            last_diagnostic: None,
        },
    )
    .expect("install skill");
    source_id
}

#[test]
fn list_workflow_agents_returns_all_built_in_runtime_agents() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);

    let response = list_workflow_agents(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ListWorkflowAgentsRequestDto {
            project_id,
            include_archived: false,
        },
    )
    .expect("list workflow agents");

    let built_in_ids: Vec<RuntimeAgentIdDto> = response
        .agents
        .iter()
        .filter_map(|agent| match &agent.r#ref {
            AgentRefDto::BuiltIn {
                runtime_agent_id, ..
            } => Some(*runtime_agent_id),
            _ => None,
        })
        .collect();

    // Ask, Plan, Engineer, Debug, Crawl, AgentCreate are always available.
    // Test agent is only available in debug/test/CI builds, which is the case here.
    for required in [
        RuntimeAgentIdDto::Ask,
        RuntimeAgentIdDto::Plan,
        RuntimeAgentIdDto::Engineer,
        RuntimeAgentIdDto::Debug,
        RuntimeAgentIdDto::Crawl,
        RuntimeAgentIdDto::AgentCreate,
    ] {
        assert!(
            built_in_ids.contains(&required),
            "expected built-in {required:?} in list, got {built_in_ids:?}",
        );
    }

    // All built-ins surface the BuiltIn scope.
    assert!(response.agents.iter().all(|agent| {
        match &agent.r#ref {
            AgentRefDto::BuiltIn { .. } => agent.scope == AgentDefinitionScopeDto::BuiltIn,
            AgentRefDto::Custom { .. } => true,
        }
    }));
}

#[test]
fn list_workflow_agents_includes_custom_definition() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let definition_id = seed_custom_definition(&repo_root);

    let response = list_workflow_agents(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ListWorkflowAgentsRequestDto {
            project_id,
            include_archived: false,
        },
    )
    .expect("list workflow agents");

    let custom = response
        .agents
        .iter()
        .find(|agent| matches!(&agent.r#ref, AgentRefDto::Custom { definition_id: id, .. } if id == &definition_id))
        .expect("custom definition appears in list");

    assert_eq!(custom.display_name, "Security Reviewer");
    assert_eq!(custom.scope, AgentDefinitionScopeDto::ProjectCustom);
}

#[test]
fn get_agent_authoring_catalog_includes_attachable_enabled_trusted_skills() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let source_id = seed_installed_skill(&repo_root, XeroSkillSourceState::Enabled);
    let unavailable_source_id = seed_installed_skill_with_id(
        &repo_root,
        "stale-rust",
        "1.0.0",
        XeroSkillSourceState::Stale,
    );

    let catalog = get_agent_authoring_catalog(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAgentAuthoringCatalogRequestDto {
            project_id,
            skill_query: None,
        },
    )
    .expect("authoring catalog");

    let entry = catalog
        .attachable_skills
        .iter()
        .find(|entry| entry.source_id == source_id)
        .expect("attachable skill entry");
    assert_eq!(entry.skill_id, "rust-best-practices");
    assert_eq!(entry.version_hash, "version-hash-rust-best-practices");
    assert_eq!(
        entry.availability_status,
        xero_desktop_lib::commands::AgentAttachedSkillAvailabilityStatusDto::Available
    );
    assert_eq!(entry.attachment["sourceId"], json!(source_id));
    assert!(catalog
        .attachable_skills
        .iter()
        .all(|entry| entry.source_id != unavailable_source_id));
    assert!(catalog.diagnostics.iter().any(|diagnostic| diagnostic.code
        == "authoring_catalog_attachable_skill_source_stale"
        && diagnostic
            .path
            .iter()
            .any(|segment| segment == &unavailable_source_id)));
}

#[test]
fn get_workflow_agent_detail_for_engineer_returns_prompts_tools_and_tables() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);

    let detail = get_workflow_agent_detail(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetWorkflowAgentDetailRequestDto {
            project_id,
            r#ref: AgentRefDto::BuiltIn {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                version: 1,
            },
        },
    )
    .expect("engineer detail");

    assert_eq!(detail.header.display_name, "Engineer");
    assert!(detail.attached_skills.is_empty());
    assert!(
        !detail.prompts.is_empty(),
        "engineer should expose system prompt"
    );
    assert!(detail.prompts[0].body.to_lowercase().contains("engineer"));

    assert!(!detail.tools.is_empty(), "engineer should expose tools");
    let tool_names: Vec<&str> = detail.tools.iter().map(|t| t.name.as_str()).collect();
    assert!(
        tool_names.contains(&"read"),
        "engineer must include read tool"
    );
    assert!(
        tool_names.contains(&"write"),
        "engineer must include write tool"
    );
    assert!(detail.tools.iter().any(|t| matches!(
        t.effect_class,
        AgentToolEffectClassDto::Write | AgentToolEffectClassDto::DestructiveWrite
    )));

    assert!(detail
        .db_touchpoints
        .writes
        .iter()
        .any(|entry| entry.table == "code_history_operations"));
    assert!(detail
        .db_touchpoints
        .writes
        .iter()
        .any(|entry| entry.table == "code_workspace_heads"));
    assert!(detail
        .db_touchpoints
        .encouraged
        .iter()
        .any(|entry| entry.table == "project_context_records"));
}

#[test]
fn get_workflow_agent_detail_for_ask_excludes_write_tools_and_code_tables() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);

    let detail = get_workflow_agent_detail(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetWorkflowAgentDetailRequestDto {
            project_id,
            r#ref: AgentRefDto::BuiltIn {
                runtime_agent_id: RuntimeAgentIdDto::Ask,
                version: 1,
            },
        },
    )
    .expect("ask detail");

    assert!(!detail.tools.iter().any(|t| matches!(
        t.effect_class,
        AgentToolEffectClassDto::Write
            | AgentToolEffectClassDto::DestructiveWrite
            | AgentToolEffectClassDto::Command
    )));
    assert!(!detail
        .db_touchpoints
        .writes
        .iter()
        .any(|entry| entry.table == "code_history_operations"));
    assert!(!detail
        .db_touchpoints
        .writes
        .iter()
        .any(|entry| entry.table == "code_workspace_heads"));
}

#[test]
fn get_workflow_agent_detail_for_custom_definition_pulls_prompt_fragments() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let definition_id = seed_custom_definition(&repo_root);

    let detail = get_workflow_agent_detail(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetWorkflowAgentDetailRequestDto {
            project_id,
            r#ref: AgentRefDto::Custom {
                definition_id: definition_id.clone(),
                version: 1,
            },
        },
    )
    .expect("custom definition detail");

    assert_eq!(detail.header.display_name, "Security Reviewer");
    assert_eq!(
        detail.header.task_purpose,
        "Audit changes against the threat model and propose mitigations."
    );

    assert!(detail.prompts.iter().any(|p| p.id == "security.coverage"));
    assert!(detail
        .prompts
        .iter()
        .any(|p| p.id == "agent_definition.workflowContract"));
    assert!(detail
        .prompts
        .iter()
        .any(|p| p.id == "agent_definition.finalResponseContract"));

    // Custom security reviewer maps to engineering profile, so it should still see code-history tables.
    assert!(detail
        .db_touchpoints
        .writes
        .iter()
        .any(|entry| entry.table == "code_history_operations"));
    assert!(detail.attached_skills.is_empty());
}

#[test]
fn get_workflow_agent_detail_for_plan_includes_decision_and_slice_sections() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);

    let detail = get_workflow_agent_detail(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetWorkflowAgentDetailRequestDto {
            project_id,
            r#ref: AgentRefDto::BuiltIn {
                runtime_agent_id: RuntimeAgentIdDto::Plan,
                version: 1,
            },
        },
    )
    .expect("plan detail");

    let decisions = detail
        .output
        .sections
        .iter()
        .find(|section| section.id == "decisions")
        .expect("plan output must include a decisions section");
    let slices = detail
        .output
        .sections
        .iter()
        .find(|section| section.id == "slices")
        .expect("plan output must include a slices section");
    assert_eq!(decisions.emphasis, AgentOutputSectionEmphasisDto::Core);
    assert_eq!(slices.emphasis, AgentOutputSectionEmphasisDto::Core);
}

#[test]
fn get_workflow_agent_detail_for_engineer_consumes_plan_pack() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);

    let detail = get_workflow_agent_detail(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetWorkflowAgentDetailRequestDto {
            project_id,
            r#ref: AgentRefDto::BuiltIn {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                version: 1,
            },
        },
    )
    .expect("engineer detail");

    assert!(
        !detail.consumes.is_empty(),
        "engineer must consume upstream artifacts"
    );
    let plan_pack = detail
        .consumes
        .iter()
        .find(|entry| entry.contract == RuntimeAgentOutputContractDto::PlanPack)
        .expect("engineer must consume the plan pack");
    assert_eq!(plan_pack.source_agent, RuntimeAgentIdDto::Plan);
    assert!(
        plan_pack.required,
        "plan pack consumption is required for engineer"
    );
    assert!(plan_pack.sections.iter().any(|s| s == "slices"));
}

#[test]
fn get_workflow_agent_detail_db_touchpoints_have_purpose_and_triggers() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);

    let detail = get_workflow_agent_detail(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetWorkflowAgentDetailRequestDto {
            project_id,
            r#ref: AgentRefDto::BuiltIn {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                version: 1,
            },
        },
    )
    .expect("engineer detail");

    let entry = detail
        .db_touchpoints
        .writes
        .iter()
        .find(|entry| entry.table == "code_history_operations")
        .expect("engineer must write code_history_operations");
    assert_eq!(entry.kind, AgentDbTouchpointKindDto::Write);
    assert!(!entry.purpose.is_empty(), "purpose must be authored");
    assert!(
        entry
            .triggers
            .iter()
            .any(|trigger| matches!(trigger, AgentTriggerRefDto::Tool { name } if name == "Edit")),
        "expected an Edit tool trigger, got {:?}",
        entry.triggers,
    );
}

#[test]
fn get_workflow_agent_detail_for_ask_consumes_nothing() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);

    let detail = get_workflow_agent_detail(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetWorkflowAgentDetailRequestDto {
            project_id,
            r#ref: AgentRefDto::BuiltIn {
                runtime_agent_id: RuntimeAgentIdDto::Ask,
                version: 1,
            },
        },
    )
    .expect("ask detail");

    assert!(
        detail.consumes.is_empty(),
        "ask should not consume upstream artifacts"
    );
}
