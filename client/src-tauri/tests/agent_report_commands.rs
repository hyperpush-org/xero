use std::fs;

use git2::{IndexAddOption, Repository, Signature};
use serde_json::json;
use tauri::Manager;
use tempfile::TempDir;
use xero_desktop_lib::{
    commands::{
        get_agent_database_touchpoint_explanation, get_agent_handoff_context_summary,
        get_agent_knowledge_inspection, get_agent_run_start_explanation,
        get_agent_support_diagnostics_bundle, get_capability_permission_explanation,
        GetAgentDatabaseTouchpointExplanationRequestDto, GetAgentHandoffContextSummaryRequestDto,
        GetAgentKnowledgeInspectionRequestDto, GetAgentRunStartExplanationRequestDto,
        GetAgentSupportDiagnosticsBundleRequestDto, GetCapabilityPermissionExplanationRequestDto,
        RuntimeAgentIdDto,
    },
    configure_builder_with_state,
    db::{self, project_store},
    git::repository::CanonicalRepository,
    registry::{self, RegistryProjectRecord},
    state::DesktopState,
};

const RUN_ID: &str = "run-agent-report-commands";

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("build mock app")
}

fn seed_project(root: &TempDir, app: &tauri::App<tauri::test::MockRuntime>) -> String {
    let repo_root = root.path().join("repo");
    fs::create_dir_all(&repo_root).expect("create repository root");
    fs::write(repo_root.join("README.md"), "# Agent report fixture\n").expect("seed readme");

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
    let project_id = format!("project-agent-reports-{fixture_id}");
    let repository_id = format!("repo-agent-reports-{fixture_id}");
    let repository = CanonicalRepository {
        project_id: project_id.clone(),
        repository_id: repository_id.clone(),
        root_path: canonical_root.clone(),
        root_path_string: root_path_string.clone(),
        common_git_dir: canonical_root.join(".git"),
        display_name: "agent-report-fixture".into(),
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

    project_store::insert_agent_run(
        &canonical_root,
        &project_store::NewAgentRunRecord {
            runtime_agent_id: RuntimeAgentIdDto::Ask,
            agent_definition_id: None,
            agent_definition_version: None,
            project_id: project_id.clone(),
            agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
            run_id: RUN_ID.into(),
            provider_id: "fixture-provider".into(),
            model_id: "fixture-model".into(),
            prompt: "Explain the agent report contracts.".into(),
            system_prompt: "You are the report fixture.".into(),
            now: "2026-07-17T20:00:00Z".into(),
        },
    )
    .expect("insert fixture agent run");

    project_id
}

#[test]
fn agent_report_commands_cover_success_validation_and_selector_routing() {
    let root = tempfile::tempdir().expect("temp dir");
    let state = DesktopState::default()
        .with_global_db_path_override(root.path().join("app-data").join("xero.db"));
    let app = build_mock_app(state);
    let project_id = seed_project(&root, &app);

    let permission =
        get_capability_permission_explanation(GetCapabilityPermissionExplanationRequestDto {
            subject_kind: "skill_runtime_tool".into(),
            subject_id: "fixture-skill".into(),
        })
        .expect("explain skill runtime permission");
    assert_eq!(permission["subjectKind"], json!("skill_runtime_tool"));
    assert_eq!(permission["subjectId"], json!("fixture-skill"));

    let touchpoints = get_agent_database_touchpoint_explanation(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAgentDatabaseTouchpointExplanationRequestDto {
            project_id: project_id.clone(),
            definition_id: "ask".into(),
            version: 1,
        },
    )
    .expect("load built-in agent touchpoints");
    assert_eq!(
        touchpoints["schema"],
        json!("xero.agent_database_touchpoint_explanation.v1")
    );

    let run_start = get_agent_run_start_explanation(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAgentRunStartExplanationRequestDto {
            project_id: project_id.clone(),
            run_id: RUN_ID.into(),
        },
    )
    .expect("load run-start explanation");
    assert_eq!(run_start["runId"], json!(RUN_ID));
    assert_eq!(run_start["model"]["providerId"], json!("fixture-provider"));

    let inspection = get_agent_knowledge_inspection(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAgentKnowledgeInspectionRequestDto {
            project_id: project_id.clone(),
            agent_session_id: None,
            run_id: Some(RUN_ID.into()),
            limit: Some(usize::MAX),
        },
    )
    .expect("inspect run-scoped knowledge");
    assert_eq!(inspection["runId"], json!(RUN_ID));
    assert_eq!(inspection["limit"], json!(50));

    let diagnostics = get_agent_support_diagnostics_bundle(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAgentSupportDiagnosticsBundleRequestDto {
            project_id: project_id.clone(),
            run_id: Some(RUN_ID.into()),
        },
    )
    .expect("load run-scoped support diagnostics");
    assert_eq!(
        diagnostics["schema"],
        json!("xero.agent_support_diagnostics_bundle.v1")
    );
    assert_eq!(diagnostics["runtimeAudit"]["status"], json!("available"));

    for request in [
        GetAgentHandoffContextSummaryRequestDto {
            project_id: project_id.clone(),
            handoff_id: Some("missing-handoff".into()),
            target_run_id: None,
            source_run_id: None,
        },
        GetAgentHandoffContextSummaryRequestDto {
            project_id: project_id.clone(),
            handoff_id: None,
            target_run_id: Some("missing-target".into()),
            source_run_id: None,
        },
        GetAgentHandoffContextSummaryRequestDto {
            project_id: project_id.clone(),
            handoff_id: None,
            target_run_id: None,
            source_run_id: Some("missing-source".into()),
        },
    ] {
        let error = get_agent_handoff_context_summary(
            app.handle().clone(),
            app.state::<DesktopState>(),
            request,
        )
        .expect_err("an unknown handoff selector must report not found");
        assert_eq!(error.code, "agent_handoff_context_summary_not_found");
    }

    let ambiguous = get_agent_handoff_context_summary(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAgentHandoffContextSummaryRequestDto {
            project_id: project_id.clone(),
            handoff_id: Some("handoff".into()),
            target_run_id: Some("target".into()),
            source_run_id: None,
        },
    )
    .expect_err("multiple handoff selectors must fail before lookup");
    assert_eq!(
        ambiguous.code,
        "agent_handoff_context_summary_identifier_ambiguous"
    );

    let missing = get_agent_handoff_context_summary(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAgentHandoffContextSummaryRequestDto {
            project_id: project_id.clone(),
            handoff_id: None,
            target_run_id: None,
            source_run_id: None,
        },
    )
    .expect_err("a handoff selector is required");
    assert_eq!(
        missing.code,
        "agent_handoff_context_summary_identifier_required"
    );

    let mismatch = get_agent_knowledge_inspection(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAgentKnowledgeInspectionRequestDto {
            project_id: project_id.clone(),
            agent_session_id: Some("different-session".into()),
            run_id: Some(RUN_ID.into()),
            limit: Some(0),
        },
    )
    .expect_err("a mismatched run and session must fail closed");
    assert_eq!(
        mismatch.code,
        "agent_knowledge_inspection_session_run_mismatch"
    );

    let unavailable_diagnostics = get_agent_support_diagnostics_bundle(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAgentSupportDiagnosticsBundleRequestDto {
            project_id: project_id.clone(),
            run_id: Some("missing-run".into()),
        },
    )
    .expect("diagnostics should embed an unavailable runtime audit");
    assert_eq!(
        unavailable_diagnostics["runtimeAudit"]["status"],
        json!("unavailable")
    );
    assert_eq!(
        unavailable_diagnostics["runtimeAudit"]["code"],
        json!("agent_run_not_found")
    );

    let invalid_permission =
        get_capability_permission_explanation(GetCapabilityPermissionExplanationRequestDto {
            subject_kind: "unknown".into(),
            subject_id: "fixture".into(),
        })
        .expect_err("unknown permission subjects must fail closed");
    assert_eq!(invalid_permission.code, "invalid_request");

    let invalid_touchpoint = get_agent_database_touchpoint_explanation(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAgentDatabaseTouchpointExplanationRequestDto {
            project_id,
            definition_id: "ask".into(),
            version: 0,
        },
    )
    .expect_err("definition version zero must be rejected");
    assert_eq!(invalid_touchpoint.code, "invalid_request");
}
